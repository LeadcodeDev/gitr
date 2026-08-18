use domain::{HeadState, RepositoryError};

use super::convert::to_domain_id;
use super::error::backend;
use super::names::branch_name;

pub(crate) fn read(repo: &gix::Repository) -> Result<HeadState, RepositoryError> {
    let head = repo.head().map_err(|err| backend("reading HEAD", err))?;
    let target = head.id().map(|id| to_domain_id(id.detach()));
    match &head.kind {
        gix::head::Kind::Unborn(name) => Ok(HeadState::Unborn {
            branch: branch_name(name.as_ref())?,
        }),
        gix::head::Kind::Symbolic(reference) => Ok(HeadState::Attached {
            branch: branch_name(reference.name.as_ref())?,
            target: target
                .ok_or_else(|| backend("HEAD branch has no target", MissingHeadTarget))?,
        }),
        gix::head::Kind::Detached { .. } => Ok(HeadState::Detached {
            target: target
                .ok_or_else(|| backend("detached HEAD has no target", MissingHeadTarget))?,
        }),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("HEAD resolved to no object")]
struct MissingHeadTarget;
