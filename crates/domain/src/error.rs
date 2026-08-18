use std::path::PathBuf;

use crate::object_id::ObjectId;

/// Why a read against a repository could not be served.
///
/// Adapters map their own failures onto these variants at the boundary. `Backend` is
/// the escape hatch for a cause the domain has no opinion about; anything a caller
/// might branch on gets its own variant instead.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("{0} is not a Git repository")]
    NotARepository(PathBuf),
    #[error("{0} does not exist or cannot be read")]
    Unreadable(PathBuf),
    #[error("repository uses an unsupported object format: {0}")]
    UnsupportedObjectFormat(String),
    #[error("object {0} was not found")]
    ObjectNotFound(ObjectId),
    #[error("reference {0} was not found")]
    ReferenceNotFound(String),
    #[error("{context}: {source}")]
    Backend {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl RepositoryError {
    pub fn backend(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Backend {
            context: context.into(),
            source: Box::new(source),
        }
    }
}

/// Why a patch could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error("object {0} was not found")]
    ObjectNotFound(ObjectId),
    #[error("git exited with status {status}: {stderr}")]
    CommandFailed { status: i32, stderr: String },
    #[error("git produced output that is not a valid patch: {0}")]
    Malformed(String),
    #[error("could not run git: {0}")]
    Unavailable(String),
}
