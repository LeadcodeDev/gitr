//! Subprocess `git` adapter.
//!
//! Resolves the login shell's `PATH`, runs `git` as a general-purpose subprocess, and
//! parses `git diff` output into [`domain::patch::Patch`] through [`SubprocessPatchReader`].

mod login_path;
mod patch_parser;
mod patch_reader;
mod runner;

pub use login_path::{LoginPathResolution, LoginPathSource, login_path};
pub use patch_reader::SubprocessPatchReader;
pub use runner::{GitOutput, GitProcessError, GitRunner};
