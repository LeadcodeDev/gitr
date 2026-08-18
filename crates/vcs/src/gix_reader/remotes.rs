use domain::{RemoteName, RepositoryError};
use gix::bstr::ByteSlice;
use gix::remote::Direction;

use super::error::backend;

pub(crate) fn read_all(repo: &gix::Repository) -> Result<Vec<domain::Remote>, RepositoryError> {
    let mut remotes = Vec::new();
    for name in repo.remote_names() {
        let remote = repo
            .find_remote(&name)
            .map_err(|err| backend(format!("reading remote {name}"), err))?;
        let fetch_url = remote
            .url(Direction::Fetch)
            .map(ToString::to_string)
            .unwrap_or_default();
        let push_url = remote
            .url(Direction::Push)
            .map(ToString::to_string)
            .unwrap_or_else(|| fetch_url.clone());
        let name = name
            .to_str()
            .map_err(|err| backend(format!("remote name {name} is not valid UTF-8"), err))?;
        remotes.push(domain::Remote {
            name: RemoteName::new(name)
                .map_err(|err| backend(format!("invalid remote name {name:?}"), err))?,
            fetch_url,
            push_url,
        });
    }
    Ok(remotes)
}
