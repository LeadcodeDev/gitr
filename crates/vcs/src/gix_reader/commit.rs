use domain::{Commit, Parents, RepositoryError};

use super::convert::{to_domain_id, to_gix_id, to_signature};
use super::error::{backend, find_commit};

pub(crate) fn read(
    repo: &gix::Repository,
    id: domain::ObjectId,
) -> Result<Commit, RepositoryError> {
    let gix_id = to_gix_id(id);
    let commit = repo
        .find_commit(gix_id)
        .map_err(|err| find_commit(gix_id, err))?;
    let message = commit
        .message()
        .map_err(|err| backend(format!("decoding message of commit {id}"), err))?;
    let author = to_signature(
        commit
            .author()
            .map_err(|err| backend(format!("decoding author of commit {id}"), err))?,
    )?;
    let committer = to_signature(
        commit
            .committer()
            .map_err(|err| backend(format!("decoding committer of commit {id}"), err))?,
    )?;
    let parent_ids: Vec<_> = commit
        .parent_ids()
        .map(|parent| to_domain_id(parent.detach()))
        .collect();

    Ok(Commit {
        id,
        parents: Parents::from_ids(&parent_ids),
        summary: message.summary().to_string(),
        body: message
            .body
            .map(|body| body.to_string())
            .unwrap_or_default(),
        author,
        committer,
    })
}
