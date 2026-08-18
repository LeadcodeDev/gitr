use domain::RepositoryError;

use super::convert::to_domain_id;
use super::error::backend;

pub(crate) fn read_all(repo: &gix::Repository) -> Result<Vec<domain::Submodule>, RepositoryError> {
    let Some(iter) = repo
        .submodules()
        .map_err(|err| backend("reading .gitmodules", err))?
    else {
        return Ok(Vec::new());
    };

    let mut submodules = Vec::new();
    for submodule in iter {
        let path = submodule.path().map_err(|err| {
            backend(
                format!("reading path of submodule {}", submodule.name()),
                err,
            )
        })?;
        let url = submodule.url().map_err(|err| {
            backend(
                format!("reading url of submodule {}", submodule.name()),
                err,
            )
        })?;
        let head = submodule
            .open()
            .map_err(|err| backend(format!("opening submodule {}", submodule.name()), err))?
            .and_then(|checkout| checkout.head_id().ok().map(|id| id.detach()))
            .map(to_domain_id);
        submodules.push(domain::Submodule {
            path: gix::path::from_bstring(path),
            url: url.to_string(),
            head,
        });
    }
    Ok(submodules)
}
