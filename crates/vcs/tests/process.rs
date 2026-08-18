//! Integration tests for the subprocess `git` runner and [`SubprocessPatchReader`],
//! run against real repositories built under [`tempfile::TempDir`].

use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use domain::error::PatchError;
use domain::object_id::ObjectId;
use domain::patch::FileStatus;
use domain::port::PatchReader;
use tempfile::TempDir;
use vcs::process::{GitProcessError, GitRunner, SubprocessPatchReader};

fn git(repository: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repository)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .expect("git must be on PATH to run these integration tests");
    assert!(status.success(), "git {args:?} failed in {repository:?}");
}

fn init_repository() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir creation cannot fail in a writable /tmp");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "core.pager", "cat"]);
    dir
}

fn commit_at(repository: &Path, date: &str, message: &str) -> ObjectId {
    let status = Command::new("git")
        .args(["commit", "-q", "-m", message])
        .current_dir(repository)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_DATE", date)
        .status()
        .expect("git commit must run");
    assert!(status.success());
    head_id(repository)
}

fn head_id(repository: &Path) -> ObjectId {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .expect("git rev-parse must run");
    assert!(output.status.success());
    let hex = String::from_utf8(output.stdout).unwrap();
    ObjectId::from_str(hex.trim()).unwrap()
}

#[test]
fn runner_returns_stdout_on_success() {
    let repository = init_repository();
    std::fs::write(repository.path().join("a.txt"), "hello\n").unwrap();
    git(repository.path(), &["add", "a.txt"]);
    commit_at(repository.path(), "2020-01-01T00:00:00", "first");

    let runner = GitRunner::new();
    let output = runner
        .run(
            repository.path(),
            &["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .unwrap();
    assert_eq!(output.stdout.trim(), head_id(repository.path()).to_string());
}

#[test]
fn runner_maps_a_failing_invocation_to_the_real_stderr() {
    let repository = init_repository();

    let runner = GitRunner::new();
    let error = runner
        .run(repository.path(), &["show", "not-a-valid-revision"])
        .unwrap_err();

    match error {
        GitProcessError::Failed { status, stderr } => {
            assert_ne!(status, 0);
            assert!(!stderr.is_empty());
        }
        GitProcessError::Spawn(source) => {
            panic!("expected a command failure, got spawn error: {source}")
        }
    }
}

#[test]
fn run_with_stderr_chunks_delivers_the_same_stderr_run_would_have_buffered() {
    let repository = init_repository();

    let runner = GitRunner::new();
    let mut collected = Vec::new();
    let output = runner
        .run_with_stderr_chunks(
            repository.path(),
            &["show", "not-a-valid-revision"],
            |chunk| collected.extend_from_slice(chunk),
        )
        .unwrap_err();

    match output {
        GitProcessError::Failed { stderr, .. } => {
            assert!(!stderr.is_empty());
            assert_eq!(String::from_utf8(collected).unwrap(), stderr);
        }
        GitProcessError::Spawn(source) => {
            panic!("expected a command failure, got spawn error: {source}")
        }
    }
}

#[test]
fn run_with_stderr_chunks_still_reports_stdout_and_success_for_a_clean_run() {
    let repository = init_repository();
    std::fs::write(repository.path().join("a.txt"), "hello\n").unwrap();
    git(repository.path(), &["add", "a.txt"]);
    commit_at(repository.path(), "2020-01-01T00:00:00", "first");

    let runner = GitRunner::new();
    let output = runner
        .run_with_stderr_chunks(
            repository.path(),
            &["rev-parse", "--verify", "--quiet", "HEAD"],
            |_| {},
        )
        .unwrap();

    assert_eq!(output.stdout.trim(), head_id(repository.path()).to_string());
}

#[test]
fn commit_patch_returns_the_expected_patch_for_a_known_commit() {
    let repository = init_repository();
    std::fs::write(repository.path().join("a.txt"), "line1\n").unwrap();
    git(repository.path(), &["add", "a.txt"]);
    commit_at(repository.path(), "2020-01-01T00:00:00", "first");

    std::fs::write(repository.path().join("a.txt"), "line1\nline2\n").unwrap();
    git(repository.path(), &["add", "a.txt"]);
    let second = commit_at(repository.path(), "2020-01-01T00:01:00", "second");

    let reader = SubprocessPatchReader::new(repository.path());
    let patch = reader.commit_patch(second).unwrap();

    assert_eq!(patch.files.len(), 1);
    let file = &patch.files[0];
    assert_eq!(file.status, FileStatus::Modified);
    assert_eq!(file.hunks.len(), 1);
    assert_eq!(patch.added_lines(), 1);
    assert_eq!(patch.deleted_lines(), 0);
}

#[test]
fn commit_patch_on_the_root_commit_is_not_blank() {
    let repository = init_repository();
    std::fs::write(repository.path().join("a.txt"), "line1\n").unwrap();
    git(repository.path(), &["add", "a.txt"]);
    let root = commit_at(repository.path(), "2020-01-01T00:00:00", "root");

    let reader = SubprocessPatchReader::new(repository.path());
    let patch = reader.commit_patch(root).unwrap();

    assert_eq!(patch.files.len(), 1);
    assert_eq!(patch.files[0].status, FileStatus::Added);
    assert_eq!(patch.added_lines(), 1);
}

#[test]
fn commit_patch_on_an_unknown_object_is_object_not_found() {
    let repository = init_repository();
    std::fs::write(repository.path().join("a.txt"), "line1\n").unwrap();
    git(repository.path(), &["add", "a.txt"]);
    commit_at(repository.path(), "2020-01-01T00:00:00", "first");

    let bogus = ObjectId::from_str("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap();
    let reader = SubprocessPatchReader::new(repository.path());
    let error = reader.commit_patch(bogus).unwrap_err();

    assert!(matches!(error, PatchError::ObjectNotFound(id) if id == bogus));
}

#[test]
fn commit_patch_on_a_merge_commit_is_the_first_parent_diff() {
    let repository = init_repository();
    let path = repository.path();
    std::fs::write(path.join("a.txt"), "line1\n").unwrap();
    git(path, &["add", "a.txt"]);
    commit_at(path, "2020-01-01T00:00:00", "root");

    git(path, &["checkout", "-q", "-b", "side"]);
    std::fs::write(path.join("b.txt"), "sidefile\n").unwrap();
    git(path, &["add", "b.txt"]);
    commit_at(path, "2020-01-01T00:01:00", "side");

    git(path, &["checkout", "-q", "main"]);
    std::fs::write(path.join("a.txt"), "line1\nline2\n").unwrap();
    git(path, &["add", "a.txt"]);
    commit_at(path, "2020-01-01T00:02:00", "second");

    let status = Command::new("git")
        .args(["merge", "-q", "--no-edit", "side"])
        .current_dir(path)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_AUTHOR_DATE", "2020-01-01T00:03:00")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_DATE", "2020-01-01T00:03:00")
        .status()
        .unwrap();
    assert!(status.success());
    let merge = head_id(path);

    let reader = SubprocessPatchReader::new(path);
    let patch = reader.commit_patch(merge).unwrap();

    assert_eq!(patch.files.len(), 1);
    assert_eq!(patch.files[0].status, FileStatus::Added);
    assert_eq!(
        patch.files[0].new_path,
        Some(std::path::PathBuf::from("b.txt"))
    );
}
