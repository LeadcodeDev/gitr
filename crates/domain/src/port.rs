use crate::commit::{Commit, CommitSummary};
use crate::error::{PatchError, RepositoryError};
use crate::object_id::ObjectId;
use crate::patch::Patch;
use crate::reference::{HeadState, RefEntry, Reference, Remote, Stash, Submodule};

/// Which tips a history walk starts from.
///
/// Mirrors the filter row above the history table: every commit reachable from any ref,
/// from local refs only, or from one named reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryScope {
    AllReferences,
    LocalReferences,
    Single(Reference),
}

/// A history read.
///
/// Order is not a parameter. A walk is always date-ordered, matching `git rev-list
/// --date-order` and GitX, and a caller has no reason to ask for anything else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRequest {
    pub scope: HistoryScope,
    /// `None` walks the whole history.
    pub limit: Option<usize>,
}

impl HistoryRequest {
    pub fn all() -> Self {
        Self {
            scope: HistoryScope::AllReferences,
            limit: None,
        }
    }

    pub fn limited(scope: HistoryScope, limit: usize) -> Self {
        Self {
            scope,
            limit: Some(limit),
        }
    }
}

/// Everything gitr reads from a repository.
///
/// Synchronous and blocking by design. gpui runs its own executor, so callers wrap
/// these in `cx.background_executor().spawn()`; making the trait async would drag in a
/// second runtime and force boxing on every call.
pub trait RepositoryReader {
    fn head(&self) -> Result<HeadState, RepositoryError>;

    /// Local branches, remote branches and tags, in one pass.
    fn references(&self) -> Result<Vec<RefEntry>, RepositoryError>;

    fn remotes(&self) -> Result<Vec<Remote>, RepositoryError>;

    fn stashes(&self) -> Result<Vec<Stash>, RepositoryError>;

    fn submodules(&self) -> Result<Vec<Submodule>, RepositoryError>;

    /// Walks history in topological order with date priority.
    fn history(&self, request: &HistoryRequest) -> Result<Vec<CommitSummary>, RepositoryError>;

    fn commit(&self, id: ObjectId) -> Result<Commit, RepositoryError>;
}

/// Produces patch text.
///
/// Split from [`RepositoryReader`] because the two have different backends: structured
/// reads go through `gix`, patch text through a `git` subprocess so the user's
/// `diff.algorithm`, `textconv` and driver configuration apply as they do in a terminal.
pub trait PatchReader {
    fn commit_patch(&self, id: ObjectId) -> Result<Patch, PatchError>;
}
