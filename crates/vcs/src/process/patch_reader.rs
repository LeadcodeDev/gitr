//! [`PatchReader`] backed by a `git` subprocess.

use std::path::PathBuf;

use domain::error::PatchError;
use domain::object_id::ObjectId;
use domain::patch::Patch;
use domain::port::PatchReader;

use super::patch_parser::parse_patch;
use super::runner::{GitProcessError, GitRunner};

/// The hash of the empty tree, constant across every Git repository. Used as the diff
/// base for a root commit, which has no parent to diff against.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Reads patch text from a repository on disk through a `git` subprocess.
pub struct SubprocessPatchReader {
    runner: GitRunner,
    repository: PathBuf,
}

impl SubprocessPatchReader {
    pub fn new(repository: impl Into<PathBuf>) -> Self {
        Self {
            runner: GitRunner::new(),
            repository: repository.into(),
        }
    }

    fn verify(&self, revspec: &str) -> Result<Option<String>, PatchError> {
        match self.runner.run(
            &self.repository,
            &["rev-parse", "--verify", "--quiet", revspec],
        ) {
            Ok(output) => Ok(Some(output.stdout.trim().to_string())),
            Err(GitProcessError::Failed { status: 1, .. }) => Ok(None),
            Err(GitProcessError::Failed { status, stderr }) => {
                Err(PatchError::CommandFailed { status, stderr })
            }
            Err(GitProcessError::Spawn(source)) => Err(PatchError::Unavailable(source.to_string())),
        }
    }

    /// The tree-ish to diff `id` against: its first parent, or the empty tree when `id`
    /// is a root commit. `git show` and `git diff-tree` both special-case a root commit
    /// differently from every other commit; diffing explicitly against a resolved base
    /// sidesteps that rather than relying on either command's default behavior.
    fn resolve_diff_base(&self, id: ObjectId) -> Result<String, PatchError> {
        let commit = id.to_string();
        if let Some(parent) = self.verify(&format!("{commit}^1"))? {
            return Ok(parent);
        }
        if self.verify(&format!("{commit}^{{commit}}"))?.is_some() {
            return Ok(EMPTY_TREE.to_string());
        }
        Err(PatchError::ObjectNotFound(id))
    }
}

impl PatchReader for SubprocessPatchReader {
    fn commit_patch(&self, id: ObjectId) -> Result<Patch, PatchError> {
        let base = self.resolve_diff_base(id)?;
        let commit = id.to_string();
        let args = [
            "--no-pager",
            "-c",
            "core.quotePath=false",
            "diff",
            "--no-color",
            "-p",
            base.as_str(),
            commit.as_str(),
        ];
        let output = self
            .runner
            .run(&self.repository, &args)
            .map_err(|error| match error {
                GitProcessError::Spawn(source) => PatchError::Unavailable(source.to_string()),
                GitProcessError::Failed { status, stderr } => {
                    PatchError::CommandFailed { status, stderr }
                }
            })?;
        parse_patch(&output.stdout)
    }
}
