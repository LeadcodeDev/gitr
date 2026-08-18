use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use domain::{Aspect, RepositoryChange, RepositoryWatcher};
use tempfile::TempDir;
use vcs::watch::FsRepositoryWatcher;

const TIMEOUT: Duration = Duration::from_secs(10);

fn git(repository: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repository)
        .env("GIT_AUTHOR_NAME", "gitr-test")
        .env("GIT_AUTHOR_EMAIL", "gitr-test@example.com")
        .env("GIT_COMMITTER_NAME", "gitr-test")
        .env("GIT_COMMITTER_EMAIL", "gitr-test@example.com")
        .status()
        .expect("git must be on PATH to run these tests");
    assert!(status.success(), "git {args:?} failed");
}

fn repository_with_one_commit() -> TempDir {
    let directory = TempDir::new().expect("tempdir");
    git(directory.path(), &["init", "-q", "-b", "main"]);
    fs::write(directory.path().join("tracked.txt"), "first\n").expect("write tracked file");
    git(directory.path(), &["add", "tracked.txt"]);
    git(directory.path(), &["commit", "-q", "-m", "initial"]);
    directory
}

fn wait_until_all_seen(receiver: &Receiver<RepositoryChange>, required: &[Aspect]) {
    let deadline = Instant::now() + TIMEOUT;
    let mut seen: Option<RepositoryChange> = None;

    loop {
        if required
            .iter()
            .all(|aspect| seen.is_some_and(|change| change.contains(*aspect)))
        {
            return;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for {required:?}, saw {seen:?}");
        }

        match receiver.recv_timeout(remaining) {
            Ok(change) => {
                seen = Some(match seen {
                    Some(existing) => existing.merge(change),
                    None => change,
                });
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for {required:?}, saw {seen:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("watcher disconnected before {required:?} arrived, saw {seen:?}")
            }
        }
    }
}

#[test]
fn a_commit_moves_a_reference() {
    let repository = repository_with_one_commit();
    let (sender, receiver) = channel();
    let watcher = FsRepositoryWatcher::new();
    let _guard = watcher
        .watch(repository.path(), sender)
        .expect("watch should start");

    fs::write(repository.path().join("tracked.txt"), "second\n").expect("write tracked file");
    git(repository.path(), &["commit", "-aq", "-m", "second"]);

    wait_until_all_seen(&receiver, &[Aspect::References]);
}

#[test]
fn checkout_b_moves_head_and_a_reference() {
    let repository = repository_with_one_commit();
    let (sender, receiver) = channel();
    let watcher = FsRepositoryWatcher::new();
    let _guard = watcher
        .watch(repository.path(), sender)
        .expect("watch should start");

    git(repository.path(), &["checkout", "-qb", "feature"]);

    wait_until_all_seen(&receiver, &[Aspect::Head, Aspect::References]);
}

#[test]
fn a_working_tree_write_is_reported() {
    let repository = repository_with_one_commit();
    let (sender, receiver) = channel();
    let watcher = FsRepositoryWatcher::new();
    let _guard = watcher
        .watch(repository.path(), sender)
        .expect("watch should start");

    fs::write(repository.path().join("tracked.txt"), "third\n").expect("write tracked file");

    wait_until_all_seen(&receiver, &[Aspect::WorkingTree]);
}

#[test]
fn dropping_the_guard_disconnects_the_channel() {
    let repository = repository_with_one_commit();
    let (sender, receiver) = channel();
    let watcher = FsRepositoryWatcher::new();
    let guard = watcher
        .watch(repository.path(), sender)
        .expect("watch should start");

    drop(guard);

    match receiver.recv_timeout(TIMEOUT) {
        Err(RecvTimeoutError::Disconnected) => {}
        other => panic!("expected the channel to disconnect, got {other:?}"),
    }
}
