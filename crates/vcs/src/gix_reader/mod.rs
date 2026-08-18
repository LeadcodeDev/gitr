//! [`domain::RepositoryReader`] on top of `gix`.
//!
//! History is walked through [`gix::traverse::commit::topo`] rather than the high-level
//! `Repository::rev_walk` builder: the latter offers only `BreadthFirst`, `ByCommitTime`
//! and `ByCommitTimeCutoff`, none of which is topological order with date priority, and
//! that ordering is what keeps a graph layout narrow.

use std::path::Path;

use domain::{
    Commit, CommitSummary, HeadState, HistoryRequest, RefEntry, RepositoryError, RepositoryReader,
};

mod commit;
mod convert;
mod error;
mod head;
mod history;
mod names;
mod references;
mod remotes;
mod stashes;
mod submodules;

/// Reads a local repository through `gix`.
///
/// Rejects, at [`open`][GixRepositoryReader::open] time, any repository whose object
/// format is not SHA-1: [`domain::ObjectId`] has no room for a SHA-256 digest, and this
/// build of `gix` has the `sha256` feature disabled, so `gix` itself already refuses to
/// open such a repository.
pub struct GixRepositoryReader {
    repo: gix::Repository,
}

impl GixRepositoryReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        let path = path.as_ref();
        let repo = gix::open(path).map_err(|err| error::open(path, err))?;
        Ok(Self { repo })
    }
}

impl RepositoryReader for GixRepositoryReader {
    fn head(&self) -> Result<HeadState, RepositoryError> {
        head::read(&self.repo)
    }

    fn references(&self) -> Result<Vec<RefEntry>, RepositoryError> {
        references::read_all(&self.repo)
    }

    fn remotes(&self) -> Result<Vec<domain::Remote>, RepositoryError> {
        remotes::read_all(&self.repo)
    }

    fn stashes(&self) -> Result<Vec<domain::Stash>, RepositoryError> {
        stashes::read_all(&self.repo)
    }

    fn submodules(&self) -> Result<Vec<domain::Submodule>, RepositoryError> {
        submodules::read_all(&self.repo)
    }

    fn history(&self, request: &HistoryRequest) -> Result<Vec<CommitSummary>, RepositoryError> {
        history::walk(&self.repo, request)
    }

    fn commit(&self, id: domain::ObjectId) -> Result<Commit, RepositoryError> {
        commit::read(&self.repo, id)
    }
}
