//! Resolves the real Git directory for a repository path.
//!
//! `.git` is a directory in an ordinary repository, but a file containing `gitdir: <path>`
//! in a linked worktree and in a submodule. Watching that file observes nothing: the file
//! itself is rewritten only at creation. The pointer must be followed before any watch is
//! registered, or the watcher looks alive while reporting nothing.

use std::path::{Path, PathBuf};

use domain::WatchError;

/// Resolves `repository`'s Git directory.
///
/// `repository` is the working tree root, not the Git directory itself. Returns
/// [`WatchError::Unreadable`] if `repository` does not exist or cannot be read, and
/// [`WatchError::NotARepository`] if it exists but has no `.git` entry, or that entry is
/// neither a directory nor a well-formed `gitdir:` pointer file.
pub(crate) fn resolve(repository: &Path) -> Result<PathBuf, WatchError> {
    if !repository.exists() {
        return Err(WatchError::Unreadable(repository.to_path_buf()));
    }

    let dot_git = repository.join(".git");
    let metadata = std::fs::symlink_metadata(&dot_git)
        .map_err(|_| WatchError::NotARepository(repository.to_path_buf()))?;

    if metadata.is_dir() {
        return Ok(dot_git);
    }

    if metadata.is_file() {
        return resolve_pointer_file(repository, &dot_git);
    }

    Err(WatchError::NotARepository(repository.to_path_buf()))
}

fn resolve_pointer_file(repository: &Path, dot_git: &Path) -> Result<PathBuf, WatchError> {
    let contents = std::fs::read_to_string(dot_git)
        .map_err(|_| WatchError::Unreadable(dot_git.to_path_buf()))?;

    let pointer = contents
        .trim()
        .strip_prefix("gitdir:")
        .ok_or_else(|| WatchError::NotARepository(repository.to_path_buf()))?
        .trim();
    let pointer = PathBuf::from(pointer);

    if pointer.is_absolute() {
        Ok(pointer)
    } else {
        Ok(dot_git.parent().unwrap_or(repository).join(pointer))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn an_ordinary_git_directory_resolves_to_itself() {
        let repository = TempDir::new().expect("tempdir");
        fs::create_dir(repository.path().join(".git")).expect("create .git");

        let resolved = resolve(repository.path()).expect("resolve");

        assert_eq!(resolved, repository.path().join(".git"));
    }

    #[test]
    fn a_gitdir_file_resolves_to_the_pointed_at_path() {
        let repository = TempDir::new().expect("tempdir");
        let real_git_dir = TempDir::new().expect("tempdir");
        fs::write(
            repository.path().join(".git"),
            format!("gitdir: {}\n", real_git_dir.path().display()),
        )
        .expect("write .git file");

        let resolved = resolve(repository.path()).expect("resolve");

        assert_eq!(resolved, real_git_dir.path());
    }

    #[test]
    fn a_relative_gitdir_pointer_resolves_against_the_dot_git_files_directory() {
        let root = TempDir::new().expect("tempdir");
        let repository = root.path().join("linked-worktree");
        fs::create_dir(&repository).expect("create worktree dir");
        fs::write(
            repository.join(".git"),
            "gitdir: ../main/.git/worktrees/linked-worktree\n",
        )
        .expect("write .git file");

        let resolved = resolve(&repository).expect("resolve");

        assert_eq!(
            resolved,
            repository.join("../main/.git/worktrees/linked-worktree")
        );
    }

    #[test]
    fn a_directory_without_a_git_entry_is_not_a_repository() {
        let repository = TempDir::new().expect("tempdir");

        let error = resolve(repository.path()).expect_err("must not resolve");

        assert!(matches!(error, WatchError::NotARepository(path) if path == repository.path()));
    }

    #[test]
    fn a_missing_path_is_unreadable() {
        let root = TempDir::new().expect("tempdir");
        let missing = root.path().join("does-not-exist");

        let error = resolve(&missing).expect_err("must not resolve");

        assert!(matches!(error, WatchError::Unreadable(path) if path == missing));
    }
}
