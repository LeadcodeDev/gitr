use std::path::Path;

use domain::RepositoryError;

const OBJECT_FORMAT_KEY: &str = "extensions.objectFormat";

pub(crate) fn open(path: &Path, source: gix::open::Error) -> RepositoryError {
    match source {
        gix::open::Error::NotARepository { path, .. } => RepositoryError::NotARepository(path),
        gix::open::Error::Config(gix::config::Error::UnsupportedObjectFormat { name }) => {
            RepositoryError::UnsupportedObjectFormat(name.to_string())
        }
        gix::open::Error::Config(gix::config::Error::ConfigTypedString(err))
            if err.key == OBJECT_FORMAT_KEY =>
        {
            RepositoryError::UnsupportedObjectFormat(
                err.value.map(|value| value.to_string()).unwrap_or_default(),
            )
        }
        other => backend(format!("opening repository at {}", path.display()), other),
    }
}

pub(crate) fn backend(
    context: impl Into<String>,
    source: impl std::error::Error + Send + Sync + 'static,
) -> RepositoryError {
    RepositoryError::backend(context, source)
}

pub(crate) fn backend_boxed(
    context: impl Into<String>,
    source: Box<dyn std::error::Error + Send + Sync>,
) -> RepositoryError {
    RepositoryError::Backend {
        context: context.into(),
        source,
    }
}

pub(crate) fn find_commit(
    id: gix::ObjectId,
    source: gix::object::find::existing::with_conversion::Error,
) -> RepositoryError {
    use gix::object::find::existing::with_conversion::Error as ConversionError;
    match source {
        ConversionError::Find(gix::object::find::existing::Error::NotFound { .. }) => {
            RepositoryError::ObjectNotFound(super::convert::to_domain_id(id))
        }
        other => backend(format!("reading commit {id}"), other),
    }
}

pub(crate) fn find_reference(
    name: &str,
    source: gix::reference::find::existing::Error,
) -> RepositoryError {
    match source {
        gix::reference::find::existing::Error::NotFound { .. } => {
            RepositoryError::ReferenceNotFound(name.to_owned())
        }
        other => backend(format!("reading reference {name}"), other),
    }
}
