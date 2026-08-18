//! A general-purpose `git` subprocess runner.
//!
//! Shaped around "run `git` with arguments in a directory and give me stdout, stderr
//! and status" rather than around any single Git operation, so milestones beyond
//! reading history and patches — checkout, stage, commit, fetch, pull, push — can call
//! [`GitRunner::run`] without a new abstraction.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use super::login_path::login_path;

/// Recorded for [`GitProcessError::Failed`] when a process was terminated by a signal.
///
/// `std::process::ExitStatus::code()` returns `None` in that case: there is no POSIX
/// exit status to report, only the fact that one is unavailable.
const SIGNAL_TERMINATED_STATUS: i32 = -1;

/// `stdout` and `stderr` from a successful `git` invocation, decoded losslessly where
/// possible and with the replacement character where the output is not valid UTF-8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

/// Why a `git` invocation did not produce output.
#[derive(Debug, thiserror::Error)]
pub enum GitProcessError {
    #[error("could not run git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git exited with status {status}: {stderr}")]
    Failed { status: i32, stderr: String },
}

/// Runs `git` with a resolved login-shell `PATH`, so the subprocess finds the same
/// credential helpers and tools the user's terminal would.
///
/// Every invocation carries `GIT_TERMINAL_PROMPT=0`. A subprocess has no controlling
/// terminal to prompt through in the first place, so without it `git` blocks on stdin
/// forever instead of failing — a hang with no error rather than the actionable
/// failure a network or clone operation needs.
pub struct GitRunner {
    path: OsString,
}

impl GitRunner {
    pub fn new() -> Self {
        Self {
            path: OsString::from(login_path().path.clone()),
        }
    }

    /// Runs `git` with `args` in `working_dir` and returns its output, or the reason it
    /// did not produce any. Blocking: waits for the child process to exit. Callers on
    /// gpui's frame thread must run this from `cx.background_executor()`.
    pub fn run(&self, working_dir: &Path, args: &[&str]) -> Result<GitOutput, GitProcessError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(working_dir)
            .env("PATH", &self.path)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(GitProcessError::Spawn)?;

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            Ok(GitOutput {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr,
            })
        } else {
            Err(GitProcessError::Failed {
                status: output.status.code().unwrap_or(SIGNAL_TERMINATED_STATUS),
                stderr,
            })
        }
    }

    /// Runs `git` like [`Self::run`], except stderr is handed to `on_stderr_chunk` as raw
    /// bytes while the process is still running, rather than only once it has exited.
    ///
    /// `Command::output` buffers everything and returns it once the child is gone, which
    /// is exactly the wrong shape for a `git clone --progress` line that overwrites
    /// itself with a carriage return: nothing would reach a caller until there was
    /// nothing left to show. This method knows nothing about that formatting — it hands
    /// over whatever a single read returned, a chunk that can start or end mid-line —
    /// and leaves reassembling lines to the caller;
    /// `crates/vcs/src/process/clone_progress.rs` is the one that does.
    ///
    /// stdout is drained on a second thread for the life of the call, whether or not the
    /// child writes anything to it: an unread, full stdout pipe would otherwise stall the
    /// child indefinitely once its OS buffer fills, which would in turn stall this method
    /// mid-read of stderr forever. That thread only moves bytes into a `Vec`, which cannot
    /// itself fail, so joining it is infallible in every case that matters here; an
    /// unexpected panic there still yields a usable (empty) stdout rather than propagating
    /// nothing useful.
    ///
    /// Blocking: waits for the child process to exit, exactly like `run`. Callers on
    /// gpui's frame thread must run this from `cx.background_executor()`.
    pub fn run_with_stderr_chunks(
        &self,
        working_dir: &Path,
        args: &[&str],
        mut on_stderr_chunk: impl FnMut(&[u8]),
    ) -> Result<GitOutput, GitProcessError> {
        let mut child = Command::new("git")
            .args(args)
            .current_dir(working_dir)
            .env("PATH", &self.path)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(GitProcessError::Spawn)?;

        let (Some(mut stdout_pipe), Some(mut stderr_pipe)) =
            (child.stdout.take(), child.stderr.take())
        else {
            return Err(GitProcessError::Spawn(io::Error::other(
                "git spawned without the stdio pipes this call requested",
            )));
        };

        let stdout_thread = thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buffer);
            buffer
        });

        let mut stderr_buffer = Vec::new();
        let mut read_buffer = [0u8; 4096];
        loop {
            match stderr_pipe.read(&mut read_buffer) {
                Ok(0) => break,
                Ok(read) => {
                    on_stderr_chunk(&read_buffer[..read]);
                    stderr_buffer.extend_from_slice(&read_buffer[..read]);
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GitProcessError::Spawn(error));
                }
            }
        }
        drop(stderr_pipe);

        let status = child.wait().map_err(GitProcessError::Spawn)?;
        let stdout_buffer = stdout_thread.join().unwrap_or_default();
        let stderr = String::from_utf8_lossy(&stderr_buffer).into_owned();

        if status.success() {
            Ok(GitOutput {
                stdout: String::from_utf8_lossy(&stdout_buffer).into_owned(),
                stderr,
            })
        } else {
            Err(GitProcessError::Failed {
                status: status.code().unwrap_or(SIGNAL_TERMINATED_STATUS),
                stderr,
            })
        }
    }
}

impl Default for GitRunner {
    fn default() -> Self {
        Self::new()
    }
}
