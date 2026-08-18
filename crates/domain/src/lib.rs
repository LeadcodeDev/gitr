//! Entities, value objects and ports for gitr.
//!
//! This crate depends on nothing but `thiserror`. It must never learn about `gix`, about
//! a subprocess, or about gpui: keeping it free of them is what lets `cargo test -p domain`
//! finish in seconds on a workspace whose full dependency tree is roughly 850 crates.

pub mod commit;
pub mod error;
pub mod object_id;
pub mod patch;
pub mod port;
pub mod reference;

pub use commit::{Commit, CommitSummary, Parents, Signature, Timestamp};
pub use error::{PatchError, RepositoryError};
pub use object_id::{HEX_LEN, ObjectId, ParseObjectIdError};
pub use patch::{DiffLine, FilePatch, FileStatus, Hunk, LineOrigin, Patch};
pub use port::{HistoryRequest, HistoryScope, PatchReader, RepositoryReader};
pub use reference::{
    BranchName, HeadState, InvalidName, RefEntry, Reference, Remote, RemoteName, Stash, Submodule,
    TagName,
};
