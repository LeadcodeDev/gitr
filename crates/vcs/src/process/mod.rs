//! Subprocess `git` adapter.
//!
//! Resolves the login shell's `PATH`, runs `git` as a general-purpose subprocess,
//! parses `git diff` output into [`domain::patch::Patch`] through [`SubprocessPatchReader`],
//! and reads a remote repository onto disk through [`GitRunner::clone_bare`] and
//! [`GitRunner::fetch`]. [`GitRunner::clone_bare_with_progress`] is the same clone with
//! [`CloneProgress`] updates streamed out as it runs.

mod clone_progress;
mod login_path;
mod patch_parser;
mod patch_reader;
mod remote;
mod runner;

pub use clone_progress::{ClonePhase, CloneProgress};
pub use login_path::{LoginPathResolution, LoginPathSource, login_path};
pub use patch_reader::SubprocessPatchReader;
pub use remote::RemoteError;
pub use runner::{GitOutput, GitProcessError, GitRunner};
