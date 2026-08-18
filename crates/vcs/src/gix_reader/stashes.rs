use domain::{RepositoryError, Stash};

use super::convert::to_domain_id;
use super::error::backend;

const STASH_REF: &str = "refs/stash";

pub(crate) fn read_all(repo: &gix::Repository) -> Result<Vec<Stash>, RepositoryError> {
    let Some(reference) = repo
        .try_find_reference(STASH_REF)
        .map_err(|err| backend("finding the stash reference", err))?
    else {
        return Ok(Vec::new());
    };

    let mut log = reference.log_iter();
    let Some(entries) = log
        .rev()
        .map_err(|err| backend("reading the stash reflog", err))?
    else {
        return Ok(Vec::new());
    };

    let mut stashes = Vec::new();
    for (index, line) in entries.enumerate() {
        let line = line.map_err(|err| backend("decoding a stash reflog entry", err))?;
        stashes.push(Stash {
            index,
            target: to_domain_id(line.new_oid),
            message: line.message.to_string(),
        });
    }
    Ok(stashes)
}
