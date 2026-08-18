//! Detects repository changes made by anything other than gitr — `git checkout`, `git
//! rebase`, `git fetch`, `git commit` run in a terminal against a repository gitr already
//! has open.
//!
//! [`FsRepositoryWatcher`] is the sole public type: an FSEvents-backed implementation of
//! [`domain::RepositoryWatcher`], built on `notify` and `notify-debouncer-full`.

mod classify;
mod git_dir;
mod watcher;

pub use watcher::{FsRepositoryWatcher, WatchGuard};
