//! A general-purpose `git` subprocess runner.
//!
//! Shaped around "run `git` with arguments in a directory and give me stdout, stderr
//! and status" rather than around any single Git operation, so milestones beyond
//! reading history and patches — checkout, stage, commit, fetch, pull, push — can call
//! [`GitRunner::run`] without a new abstraction.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

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
}

impl Default for GitRunner {
    fn default() -> Self {
        Self::new()
    }
}
