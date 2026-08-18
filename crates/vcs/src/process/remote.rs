//! Bare partial clone and fetch through the `git` subprocess, so pasting a repository
//! URL turns it into an ordinary repository on disk — the existing `gix` reader and the
//! `graph` layout then work on it unchanged, with no forge API, no token and no
//! rate limit involved.
//!
//! Both operations block the calling thread for as long as `git` runs. Callers on
//! gpui's frame thread must run them from `cx.background_executor()`, exactly like
//! [`super::GitRunner::run`] itself.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use super::clone_progress::{CloneProgress, ProgressLineSplitter, parse_progress_line};
use super::runner::{GitProcessError, GitRunner};

const PARTIAL_CLONE_SUFFIX: &str = ".gitr-partial";

const NOT_FOUND_PATTERNS: [&str; 2] = ["does not exist", "repository not found"];

const AUTHENTICATION_PATTERNS: [&str; 6] = [
    "terminal prompts disabled",
    "could not read username",
    "could not read password",
    "authentication failed",
    "permission denied (publickey)",
    "invalid username or",
];

const NETWORK_PATTERNS: [&str; 5] = [
    "could not resolve host",
    "could not resolve hostname",
    "network is unreachable",
    "connection refused",
    "connection timed out",
];

/// Why a [`GitRunner::clone_bare`] or [`GitRunner::fetch`] did not complete.
///
/// `git` reports every failure the same way — a nonzero exit and a stderr string —
/// but a URL that resolves to nothing, one that exists behind credentials gitr does
/// not provide, and a machine with no route to the remote at all are three different
/// situations a user needs told apart. There is no structured error code to switch on,
/// so classification pattern-matches `git`'s own stderr text; every variant that can
/// result from a `git` failure keeps that stderr so a caller can fall back to it
/// verbatim when the heuristic guesses wrong.
///
/// GitHub in particular collapses "does not exist" and "is private" into the identical
/// stderr pre-authentication — the server will not confirm a private repository's
/// existence to an unauthenticated caller. That message lands on
/// [`RemoteError::AuthenticationRequired`], not [`RemoteError::NotFound`]: it is the
/// more honest read for a repository the user might actually own, and it is the only
/// one of the two that tells them what to do next.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("{0} already exists")]
    DestinationExists(PathBuf),
    #[error("not found: {stderr}")]
    NotFound { stderr: String },
    #[error("authentication required: {stderr}")]
    AuthenticationRequired { stderr: String },
    #[error("network unavailable: {stderr}")]
    NetworkUnavailable { stderr: String },
    #[error("git exited with status {status}: {stderr}")]
    Failed { status: i32, stderr: String },
    #[error("could not run git: {0}")]
    Unavailable(String),
    #[error("filesystem operation on {path} failed: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "{cause} (the partial clone at {path} could not be removed afterward: {cleanup_source})"
    )]
    CleanupFailed {
        cause: Box<RemoteError>,
        path: PathBuf,
        #[source]
        cleanup_source: io::Error,
    },
}

fn classify(error: GitProcessError) -> RemoteError {
    match error {
        GitProcessError::Spawn(source) => RemoteError::Unavailable(source.to_string()),
        GitProcessError::Failed { status, stderr } => classify_stderr(status, stderr),
    }
}

fn classify_stderr(status: i32, stderr: String) -> RemoteError {
    let lower = stderr.to_lowercase();
    if matches_any(&lower, &NOT_FOUND_PATTERNS) {
        RemoteError::NotFound { stderr }
    } else if matches_any(&lower, &AUTHENTICATION_PATTERNS) {
        RemoteError::AuthenticationRequired { stderr }
    } else if matches_any(&lower, &NETWORK_PATTERNS) {
        RemoteError::NetworkUnavailable { stderr }
    } else {
        RemoteError::Failed { status, stderr }
    }
}

fn matches_any(haystack: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| haystack.contains(pattern))
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn partial_clone_name(destination: &Path) -> OsString {
    let mut name = destination
        .file_name()
        .map(OsStr::to_os_string)
        .unwrap_or_else(|| OsString::from("clone"));
    name.push(PARTIAL_CLONE_SUFFIX);
    name
}

impl GitRunner {
    /// Clones `url` as a bare, blob-filtered partial clone into `destination`.
    ///
    /// Commits and trees are fetched in full, so history and the graph layout work
    /// entirely offline; blob content is fetched lazily, the first time something
    /// actually opens a diff. Measured on `rust-lang/cargo` (23 789 commits): 5.4 s and
    /// 25 MB against 17.4 s and 78 MB for an unfiltered bare clone. Against a local
    /// path — as this crate's tests exercise, to stay off the network — `git`'s local
    /// transport does not implement the filtering protocol and clones every object
    /// regardless, printing `warning: --filter is ignored in local clones` or
    /// `warning: filtering not recognized by server, ignoring`; the requested filter is
    /// still recorded in the clone's config (`remote.origin.partialclonefilter`), it
    /// simply has nothing to act on until the remote is a real server.
    ///
    /// `destination` must not already exist: [`RemoteError::DestinationExists`] if it
    /// does, so refreshing a clone already on disk goes through [`GitRunner::fetch`]
    /// instead of this method risking an accidental overwrite. The clone itself lands
    /// in a sibling directory named `<destination file name>.gitr-partial` and is
    /// renamed into place only after `git` exits successfully, so a crash mid-clone —
    /// this process killed, the machine losing power — never leaves anything readable
    /// at `destination`; the next `clone_bare` call for the same `destination` removes
    /// that sibling before cloning again.
    ///
    /// Blocking: runs `git clone` to completion. Call from `cx.background_executor()`.
    pub fn clone_bare(&self, url: &str, destination: &Path) -> Result<(), RemoteError> {
        self.clone_bare_internal(url, destination, None)
    }

    /// Clones exactly like [`Self::clone_bare`], reporting each phase and percentage
    /// change `git` prints along the way through `progress`.
    ///
    /// A [`Sender`] rather than a callback: this call already runs inside whatever
    /// closure a caller handed to `cx.background_executor().spawn()`, so a callback would
    /// need the same `Send + 'static` shape a channel already has for free, with none of
    /// the upside — `crates/ui/src/repository/state.rs`'s filesystem watcher already
    /// crosses from a blocking background thread into gpui this way, and its caller
    /// already knows how to drain one, so `crates/ui/src/workspace.rs` reuses that same
    /// shape rather than inventing a second one next to it. It also keeps this crate
    /// ignorant of gpui: a `Sender` is a plain channel that does not care who, if anyone,
    /// is still receiving.
    ///
    /// Every update is best-effort: a send that fails because the receiving end has
    /// already been dropped is not a reason to fail or slow the clone itself, so it is
    /// silently discarded here — the clone's own success or failure is still reported
    /// through the returned `Result`, unaffected by whether anyone was watching.
    ///
    /// Blocking: runs `git clone` to completion. Call from `cx.background_executor()`.
    pub fn clone_bare_with_progress(
        &self,
        url: &str,
        destination: &Path,
        progress: Sender<CloneProgress>,
    ) -> Result<(), RemoteError> {
        self.clone_bare_internal(url, destination, Some(progress))
    }

    fn clone_bare_internal(
        &self,
        url: &str,
        destination: &Path,
        progress: Option<Sender<CloneProgress>>,
    ) -> Result<(), RemoteError> {
        if destination.exists() {
            return Err(RemoteError::DestinationExists(destination.to_path_buf()));
        }

        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| RemoteError::Io {
            path: parent.to_path_buf(),
            source,
        })?;

        let temp = parent.join(partial_clone_name(destination));
        remove_if_present(&temp).map_err(|source| RemoteError::Io {
            path: temp.clone(),
            source,
        })?;

        let temp_display = temp.to_string_lossy().into_owned();
        let args = [
            "clone",
            "--bare",
            "--filter=blob:none",
            "--progress",
            url,
            &temp_display,
        ];
        let outcome = match progress {
            Some(sender) => self.run_clone_streaming(parent, &args, &sender),
            None => self.run(parent, &args).map(|_| ()),
        };

        if let Err(error) = outcome {
            let cause = classify(error);
            return Err(match remove_if_present(&temp) {
                Ok(()) => cause,
                Err(cleanup_source) => RemoteError::CleanupFailed {
                    cause: Box::new(cause),
                    path: temp,
                    cleanup_source,
                },
            });
        }

        fs::rename(&temp, destination).map_err(|source| RemoteError::Io {
            path: destination.to_path_buf(),
            source,
        })
    }

    /// Reassembles `git`'s `\r`-and-`\n`-delimited progress lines out of the raw stderr
    /// chunks [`GitRunner::run_with_stderr_chunks`] hands over as they arrive, and sends
    /// every one [`parse_progress_line`] recognises.
    fn run_clone_streaming(
        &self,
        working_dir: &Path,
        args: &[&str],
        progress: &Sender<CloneProgress>,
    ) -> Result<(), GitProcessError> {
        let mut splitter = ProgressLineSplitter::new();
        self.run_with_stderr_chunks(working_dir, args, |chunk| {
            for line in splitter.feed(chunk) {
                if let Some(update) = parse_progress_line(&line) {
                    let _ = progress.send(update);
                }
            }
        })
        .map(|_| ())
    }

    /// Updates `repository`, a bare clone previously produced by [`GitRunner::clone_bare`].
    ///
    /// `git clone --bare` records no fetch refspec on the resulting `remote.origin`, so
    /// a plain `git fetch` there only updates `FETCH_HEAD` and touches no branch. The
    /// refspecs are therefore passed explicitly: `refs/heads/*` and `refs/tags/*`, both
    /// mirrored one-to-one and both pruned, so a branch or tag deleted on the remote
    /// disappears here too rather than accumulating forever. Tags do come along, on the
    /// same terms as branches.
    ///
    /// Blocking: runs `git fetch` to completion. Call from `cx.background_executor()`.
    pub fn fetch(&self, repository: &Path) -> Result<(), RemoteError> {
        self.run(
            repository,
            &[
                "fetch",
                "--prune",
                "--prune-tags",
                "origin",
                "+refs/heads/*:refs/heads/*",
                "refs/tags/*:refs/tags/*",
            ],
        )
        .map(|_| ())
        .map_err(classify)
    }
}
