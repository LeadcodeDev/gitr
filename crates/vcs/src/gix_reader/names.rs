use domain::{BranchName, Reference, RemoteName, RepositoryError, TagName};
use gix::bstr::{BStr, ByteSlice};

use super::error::backend;

pub(crate) fn branch_name(full: &gix::refs::FullNameRef) -> Result<BranchName, RepositoryError> {
    let short = decode(full.shorten(), full)?;
    BranchName::new(short).map_err(|err| backend(format!("invalid branch name {short:?}"), err))
}

pub(crate) fn tag_name(full: &gix::refs::FullNameRef) -> Result<TagName, RepositoryError> {
    let short = decode(full.shorten(), full)?;
    TagName::new(short).map_err(|err| backend(format!("invalid tag name {short:?}"), err))
}

pub(crate) fn remote_branch(
    short: &BStr,
    full: &gix::refs::FullNameRef,
) -> Result<Reference, RepositoryError> {
    let short = decode(short, full)?;
    let (remote, branch) = short.split_once('/').ok_or_else(|| {
        backend(
            format!("remote branch {full} has no remote segment"),
            MissingRemoteSegment,
        )
    })?;
    let remote = RemoteName::new(remote)
        .map_err(|err| backend(format!("invalid remote name {remote:?}"), err))?;
    let branch = BranchName::new(branch)
        .map_err(|err| backend(format!("invalid branch name {branch:?}"), err))?;
    Ok(Reference::RemoteBranch { remote, branch })
}

fn decode<'a>(short: &'a BStr, full: &gix::refs::FullNameRef) -> Result<&'a str, RepositoryError> {
    short
        .to_str()
        .map_err(|err| backend(format!("reference name {full} is not valid UTF-8"), err))
}

#[derive(Debug, thiserror::Error)]
#[error("expected <remote>/<branch>")]
struct MissingRemoteSegment;
