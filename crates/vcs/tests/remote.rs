//! Integration tests for [`GitRunner::clone_bare`] and [`GitRunner::fetch`], exercised
//! against a local source repository under [`tempfile::TempDir`] so the whole code path
//! runs with no network, no flakiness and no dependency on a remote host being
//! reachable.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use vcs::process::{GitRunner, RemoteError};

fn git(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git must be on PATH to run these integration tests");
    assert!(
        output.status.success(),
        "git {args:?} failed in {repository:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repository() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir creation cannot fail in a writable /tmp");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    dir
}

fn commit(repository: &Path, message: &str) -> String {
    Command::new("git")
        .args(["commit", "-q", "--allow-empty", "-m", message])
        .current_dir(repository)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git commit must run");
    git(repository, &["rev-parse", "HEAD"])
}

fn head_of(repository: &Path, reference: &str) -> String {
    git(repository, &["rev-parse", reference])
}

#[test]
fn clone_bare_produces_the_expected_commits() {
    let source = init_repository();
    commit(source.path(), "first");
    let second = commit(source.path(), "second");
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");

    GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap();

    assert!(destination.join("HEAD").is_file());
    assert_eq!(head_of(&destination, "HEAD"), second);
    assert_eq!(head_of(&destination, "refs/heads/main"), second);
}

#[test]
fn clone_bare_requests_a_blob_none_partial_clone() {
    let source = init_repository();
    commit(source.path(), "first");
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");

    GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap();

    let filter = git(
        &destination,
        &["config", "--get", "remote.origin.partialclonefilter"],
    );
    assert_eq!(filter, "blob:none");
}

#[test]
fn clone_bare_refuses_to_overwrite_an_existing_destination() {
    let source = init_repository();
    commit(source.path(), "first");
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("marker"), b"pre-existing").unwrap();

    let error = GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap_err();

    match error {
        RemoteError::DestinationExists(path) => assert_eq!(path, destination),
        other => panic!("expected DestinationExists, got {other:?}"),
    }
    assert!(destination.join("marker").is_file());
}

#[test]
fn clone_bare_on_a_nonexistent_source_fails_as_not_found_and_leaves_nothing_behind() {
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");

    let error = GitRunner::new()
        .clone_bare("/definitely/not/a/real/source/path/xyz", &destination)
        .unwrap_err();

    assert!(
        matches!(error, RemoteError::NotFound { .. }),
        "expected NotFound, got {error:?}"
    );
    assert!(!destination.exists());
    let partial = workspace.path().join("dest.git.gitr-partial");
    assert!(!partial.exists());
}

#[test]
fn clone_bare_supersedes_a_stale_partial_clone_left_by_a_previous_crash() {
    let source = init_repository();
    let first = commit(source.path(), "first");
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");
    let stale_partial = workspace.path().join("dest.git.gitr-partial");
    std::fs::create_dir(&stale_partial).unwrap();
    std::fs::write(stale_partial.join("garbage"), b"not a repository").unwrap();

    GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap();

    assert!(!stale_partial.exists());
    assert_eq!(head_of(&destination, "HEAD"), first);
}

#[test]
fn fetch_picks_up_a_commit_added_to_the_source_afterward() {
    let source = init_repository();
    commit(source.path(), "first");
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");
    GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap();

    let second = commit(source.path(), "second");
    GitRunner::new().fetch(&destination).unwrap();

    assert_eq!(head_of(&destination, "refs/heads/main"), second);
}

#[test]
fn fetch_prunes_a_branch_deleted_on_the_source() {
    let source = init_repository();
    commit(source.path(), "first");
    git(source.path(), &["branch", "feature"]);
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");
    GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap();
    assert!(!head_of(&destination, "refs/heads/feature").is_empty());

    git(source.path(), &["branch", "-D", "feature"]);
    GitRunner::new().fetch(&destination).unwrap();

    let refs = git(&destination, &["for-each-ref", "refs/heads/feature"]);
    assert_eq!(refs, "");
}

#[test]
fn fetch_brings_new_tags_and_prunes_deleted_ones() {
    let source = init_repository();
    commit(source.path(), "first");
    git(source.path(), &["tag", "v1.0"]);
    let workspace = tempfile::tempdir().unwrap();
    let destination = workspace.path().join("dest.git");
    GitRunner::new()
        .clone_bare(&source.path().to_string_lossy(), &destination)
        .unwrap();
    assert!(!git(&destination, &["for-each-ref", "refs/tags/v1.0"]).is_empty());

    git(source.path(), &["tag", "-d", "v1.0"]);
    git(source.path(), &["tag", "v2.0"]);
    GitRunner::new().fetch(&destination).unwrap();

    assert_eq!(git(&destination, &["for-each-ref", "refs/tags/v1.0"]), "");
    assert!(!git(&destination, &["for-each-ref", "refs/tags/v2.0"]).is_empty());
}
