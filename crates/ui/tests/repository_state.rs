//! Proves that `repository::state`'s load path actually reads a real repository.
//!
//! `gpui::TestAppContext` is what constructing a `Context<RepositoryState>` — and so
//! driving `RepositoryState::open`/`select`/`reload` themselves — would need. It is
//! compiled into `gpui` only behind `#[cfg(any(test, feature = "test-support"))]`
//! (`gpui/src/gpui.rs`), and `crates/ui/Cargo.toml`'s `gpui` dependency does not enable
//! `test-support`. Confirmed empirically before writing this file: a probe test with
//! `use gpui::TestAppContext;` and `#[gpui::test]` under this directory fails with
//! `unresolved import `gpui::TestAppContext`` and `cannot find function `run_test` in
//! crate `gpui``. Enabling it needs `gpui = { workspace = true, features =
//! ["test-support"] }` under `[dev-dependencies]` in `crates/ui/Cargo.toml`, which is a
//! change out of this workstream's scope (`Cargo.toml` files are not this workstream's
//! to edit).
//!
//! So these tests call the free functions `read_head`, `read_references`, `read_history`
//! and `read_commit_detail` directly — the entire load path, made `pub` in
//! `crates/ui/src/repository/state.rs` for exactly this — against repositories built
//! with `tempfile::TempDir` and `git`, the same way `crates/vcs/tests/gix_reader.rs`
//! does. The watcher test drives `vcs::watch::FsRepositoryWatcher` directly too, the way
//! `crates/vcs/tests/watch.rs` does: `RepositoryState::reload` is a thin dispatch on top
//! of that watcher and of `reload_targets`, and both are already covered elsewhere — the
//! watcher by `vcs`'s own tests, the dispatch decision by `state.rs`'s synthetic unit
//! tests. What neither covers, and what that test proves, is that a real commit produces
//! a real signal and that re-reading afterward actually observes it.

use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use domain::{
    Aspect, BranchName, HeadState, HistoryRequest, ObjectId, Reference, RemoteName,
    RepositoryChange, RepositoryError, RepositoryWatcher,
};
use tempfile::TempDir;
use ui::repository::ReferenceIndex;
use ui::repository::state::{read_commit_detail, read_head, read_history, read_references};
use vcs::watch::FsRepositoryWatcher;

const BASE: i64 = 1_700_000_000;
const WATCH_TIMEOUT: Duration = Duration::from_secs(10);

fn run(dir: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> String {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test Author")
        .env("GIT_AUTHOR_EMAIL", "author@example.com")
        .env("GIT_COMMITTER_NAME", "Test Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@example.com")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let output = command.output().expect("git must be installed and on PATH");
    assert!(
        output.status.success(),
        "git {args:?} in {} failed:\n{}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_at(dir: &Path, args: &[&str], seconds: i64) -> String {
    let date = format!("{seconds} +0000");
    run(
        dir,
        args,
        &[
            ("GIT_AUTHOR_DATE", date.as_str()),
            ("GIT_COMMITTER_DATE", date.as_str()),
        ],
    )
}

fn init_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    run(
        dir.path(),
        &["init", "--quiet", "--initial-branch=main"],
        &[],
    );
    dir
}

fn commit(dir: &Path, message: &str, seconds: i64) -> ObjectId {
    git_at(
        dir,
        &["commit", "--quiet", "--allow-empty", "-m", message],
        seconds,
    );
    rev_parse(dir, "HEAD")
}

fn rev_parse(dir: &Path, rev: &str) -> ObjectId {
    run(dir, &["rev-parse", rev], &[])
        .parse()
        .expect("git rev-parse must print a valid object id")
}

fn wait_for_a_head_or_references_change(receiver: &Receiver<RepositoryChange>) -> RepositoryChange {
    let deadline = Instant::now() + WATCH_TIMEOUT;
    let mut seen: Option<RepositoryChange> = None;
    loop {
        if let Some(change) = seen
            && (change.contains(Aspect::Head) || change.contains(Aspect::References))
        {
            return change;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for a Head or References change, saw {seen:?}");
        }
        match receiver.recv_timeout(remaining) {
            Ok(change) => {
                seen = Some(match seen {
                    Some(existing) => existing.merge(change),
                    None => change,
                });
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for a Head or References change, saw {seen:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!(
                    "watcher disconnected before a Head or References change arrived, saw {seen:?}"
                )
            }
        }
    }
}

#[test]
fn history_reaches_a_topologically_ordered_result_with_a_matching_graph_layout() {
    let dir = init_repo();
    let c1 = commit(dir.path(), "c1", BASE + 100);
    let c2 = commit(dir.path(), "c2", BASE + 200);
    let c3 = commit(dir.path(), "c3", BASE + 300);

    let (commits, graph_layout) =
        read_history(dir.path(), &HistoryRequest::all()).expect("history reads");

    let ids: Vec<_> = commits.iter().map(|commit| commit.id).collect();
    assert_eq!(ids, vec![c3, c2, c1]);
    assert_eq!(graph_layout.rows.len(), commits.len());
}

#[test]
fn references_are_grouped_into_local_remote_and_tag_buckets() {
    let bare = tempfile::tempdir().expect("tempdir");
    run(
        bare.path(),
        &["init", "--quiet", "--bare", "--initial-branch=main"],
        &[],
    );

    let work = init_repo();
    let c1 = commit(work.path(), "c1", BASE);
    run(
        work.path(),
        &[
            "remote",
            "add",
            "origin",
            bare.path().to_str().expect("utf8 path"),
        ],
        &[],
    );
    run(work.path(), &["push", "--quiet", "origin", "main"], &[]);
    run(work.path(), &["fetch", "--quiet", "origin"], &[]);
    run(work.path(), &["tag", "v1.0.0"], &[]);

    let entries = read_references(work.path()).expect("references read");
    let index = ReferenceIndex::from_entries(entries);

    assert_eq!(
        index.local_branches.len(),
        1,
        "expected only main: {index:?}"
    );
    assert_eq!(index.local_branches[0].target, c1);

    let origin_main = Reference::RemoteBranch {
        remote: RemoteName::new("origin").expect("name"),
        branch: BranchName::new("main").expect("name"),
    };
    let remote_main = index
        .remote_branches
        .iter()
        .find(|entry| entry.reference == origin_main)
        .unwrap_or_else(|| {
            panic!(
                "origin/main must be reported, got {:?}",
                index.remote_branches
            )
        });
    assert_eq!(remote_main.target, c1);

    assert_eq!(index.tags.len(), 1);
    assert_eq!(index.tags[0].target, c1);
}

#[test]
fn head_reports_the_attached_branch_and_its_target() {
    let dir = init_repo();
    let c1 = commit(dir.path(), "c1", BASE);

    let head = read_head(dir.path()).expect("head reads");
    assert_eq!(
        head,
        HeadState::Attached {
            branch: BranchName::new("main").expect("branch name"),
            target: c1,
        }
    );
}

#[test]
fn head_is_unborn_on_a_freshly_initialised_repository_rather_than_a_failure() {
    let dir = init_repo();

    let head = read_head(dir.path()).expect("head reads even with no commits yet");
    assert_eq!(
        head,
        HeadState::Unborn {
            branch: BranchName::new("main").expect("branch name"),
        }
    );
}

#[test]
fn commit_detail_reads_the_commit_and_a_non_empty_patch() {
    let dir = init_repo();
    std::fs::write(dir.path().join("file.txt"), "a\n").expect("write file");
    run(dir.path(), &["add", "file.txt"], &[]);
    let c1 = commit(dir.path(), "add file", BASE);

    let detail = read_commit_detail(dir.path(), c1).expect("detail reads");

    assert_eq!(detail.commit.id, c1);
    assert_eq!(detail.commit.summary, "add file");
    assert!(
        !detail.patch.files.is_empty(),
        "a commit that adds a file must produce a non-empty patch"
    );
}

#[test]
fn opening_a_non_repository_path_fails_history_without_panicking() {
    let dir = tempfile::tempdir().expect("tempdir");

    let result = read_history(dir.path(), &HistoryRequest::all());

    assert!(matches!(result, Err(RepositoryError::NotARepository(_))));
}

#[test]
fn a_commit_made_after_watching_starts_is_seen_by_a_fresh_history_read() {
    let dir = init_repo();
    commit(dir.path(), "c1", BASE);

    let (before, _) = read_history(dir.path(), &HistoryRequest::all()).expect("history reads");
    assert_eq!(before.len(), 1);

    let (sender, receiver) = channel();
    let watcher = FsRepositoryWatcher::new();
    let _guard = watcher
        .watch(dir.path(), sender)
        .expect("watch should start");

    let c2 = commit(dir.path(), "c2", BASE + 100);

    let change = wait_for_a_head_or_references_change(&receiver);
    assert!(
        change.contains(Aspect::Head) || change.contains(Aspect::References),
        "a commit on the checked-out branch must report Head or References, saw {change:?}"
    );

    let (after, _) = read_history(dir.path(), &HistoryRequest::all()).expect("history re-reads");
    let ids: Vec<_> = after.iter().map(|commit| commit.id).collect();
    assert_eq!(after.len(), 2);
    assert_eq!(
        ids[0], c2,
        "a fresh read taken after the watcher fires must see the new commit tip-first"
    );
}
