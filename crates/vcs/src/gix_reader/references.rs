use domain::{RefEntry, Reference, RepositoryError};
use gix::refs::Category;

use super::convert::to_domain_id;
use super::error::{backend, backend_boxed};
use super::names::{branch_name, remote_branch, tag_name};

pub(crate) fn read_all(repo: &gix::Repository) -> Result<Vec<RefEntry>, RepositoryError> {
    let platform = repo
        .references()
        .map_err(|err| backend("listing references", err))?;
    let iter = platform
        .all()
        .map_err(|err| backend("listing references", err))?;

    let mut entries = Vec::new();
    for reference in iter {
        let mut reference = reference.map_err(|err| backend_boxed("reading a reference", err))?;
        let Some((category, short)) = reference.name().category_and_short_name() else {
            continue;
        };
        let full = reference.name().to_owned();
        let reference_value = match category {
            Category::LocalBranch => Reference::LocalBranch(branch_name(full.as_ref())?),
            Category::RemoteBranch => remote_branch(short, full.as_ref())?,
            Category::Tag => Reference::Tag(tag_name(full.as_ref())?),
            _ => continue,
        };
        let upstream = match &reference_value {
            Reference::LocalBranch(_) => upstream_of(&reference)?,
            _ => None,
        };
        let target = to_domain_id(
            reference
                .peel_to_id()
                .map_err(|err| backend(format!("resolving reference {full}"), err))?
                .detach(),
        );
        entries.push(RefEntry {
            reference: reference_value,
            target,
            upstream,
        });
    }
    Ok(entries)
}

fn upstream_of(reference: &gix::Reference<'_>) -> Result<Option<Reference>, RepositoryError> {
    let Some(tracking) = reference.remote_tracking_ref_name(gix::remote::Direction::Fetch) else {
        return Ok(None);
    };
    let tracking = tracking
        .map_err(|err| backend(format!("resolving upstream of {}", reference.name()), err))?;
    let Some((Category::RemoteBranch, short)) = tracking.as_ref().category_and_short_name() else {
        return Ok(None);
    };
    Ok(Some(remote_branch(short, tracking.as_ref())?))
}
