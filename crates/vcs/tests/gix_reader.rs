use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use domain::{
    BranchName, HeadState, HistoryRequest, HistoryScope, Parents, Reference, RemoteName,
    RepositoryError, RepositoryReader, TagName,
};
use tempfile::TempDir;
use vcs::gix_reader::GixRepositoryReader;

const BASE: i64 = 1_700_000_000;

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

fn clone_repo(src: &Path, dest: &Path) {
    let output = Command::new("git")
        .arg("clone")
        .arg("--quiet")
        .arg(src)
        .arg(dest)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git must be installed and on PATH");
    assert!(
        output.status.success(),
        "git clone {} {} failed:\n{}",
        src.display(),
        dest.display(),
        String::from_utf8_lossy(&output.stderr)
    );
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

fn commit(dir: &Path, message: &str, seconds: i64) -> domain::ObjectId {
    git_at(
        dir,
        &["commit", "--quiet", "--allow-empty", "-m", message],
        seconds,
    );
    rev_parse(dir, "HEAD")
}

fn rev_parse(dir: &Path, rev: &str) -> domain::ObjectId {
    run(dir, &["rev-parse", rev], &[])
        .parse()
        .expect("git rev-parse must print a valid object id")
}

#[test]
fn linear_history_is_walked_tip_first() {
    let dir = init_repo();
    let c1 = commit(dir.path(), "c1", BASE + 100);
    let c2 = commit(dir.path(), "c2", BASE + 200);
    let c3 = commit(dir.path(), "c3", BASE + 300);

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    let history = reader.history(&HistoryRequest::all()).expect("history");

    let ids: Vec<_> = history.iter().map(|entry| entry.id).collect();
    assert_eq!(ids, vec![c3, c2, c1]);
    assert_eq!(history[0].parents, Parents::Linear(c2));
    assert_eq!(history[1].parents, Parents::Linear(c1));
    assert_eq!(history[2].parents, Parents::Root);
    assert_eq!(history[0].summary, "c3");
}

#[test]
fn merge_commit_reports_parents_first_then_second() {
    let dir = init_repo();
    commit(dir.path(), "base", BASE);
    run(dir.path(), &["branch", "feature"], &[]);
    let main_tip = commit(dir.path(), "main-tip", BASE + 100);
    run(dir.path(), &["checkout", "--quiet", "feature"], &[]);
    let feature_tip = commit(dir.path(), "feature-tip", BASE + 100);
    git_at(
        dir.path(),
        &["merge", "--no-ff", "--quiet", "-m", "merge", "main"],
        BASE + 300,
    );
    let merge_id = rev_parse(dir.path(), "HEAD");

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    let merge_commit = reader.commit(merge_id).expect("commit");

    assert_eq!(merge_commit.parents, Parents::Merge(feature_tip, main_tip));
}

#[test]
fn unborn_head_on_a_freshly_initialised_repository() {
    let dir = init_repo();
    let reader = GixRepositoryReader::open(dir.path()).expect("open");

    assert_eq!(
        reader.head().expect("head"),
        HeadState::Unborn {
            branch: BranchName::new("main").expect("branch name")
        }
    );
}

#[test]
fn detached_head_reports_its_target() {
    let dir = init_repo();
    let c1 = commit(dir.path(), "c1", BASE);
    run(dir.path(), &["checkout", "--quiet", &c1.to_string()], &[]);

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    assert_eq!(
        reader.head().expect("head"),
        HeadState::Detached { target: c1 }
    );
}

#[test]
fn local_branch_remote_branch_and_upstream_are_all_reported() {
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
    run(
        work.path(),
        &["branch", "--quiet", "--set-upstream-to=origin/main", "main"],
        &[],
    );

    let reader = GixRepositoryReader::open(work.path()).expect("open");
    let refs = reader.references().expect("references");

    let local = refs
        .iter()
        .find(|entry| {
            entry.reference == Reference::LocalBranch(BranchName::new("main").expect("name"))
        })
        .expect("local branch main is listed");
    assert_eq!(local.target, c1);
    assert_eq!(
        local.upstream,
        Some(Reference::RemoteBranch {
            remote: RemoteName::new("origin").expect("name"),
            branch: BranchName::new("main").expect("name"),
        })
    );

    let remote = refs
        .iter()
        .find(|entry| {
            matches!(
                &entry.reference,
                Reference::RemoteBranch { remote, branch }
                    if remote.as_str() == "origin" && branch.as_str() == "main"
            )
        })
        .expect("remote branch origin/main is listed");
    assert_eq!(remote.target, c1);
    assert_eq!(remote.upstream, None);
}

#[test]
fn lightweight_and_annotated_tags_resolve_to_their_commit() {
    let dir = init_repo();
    let c1 = commit(dir.path(), "c1", BASE);
    run(dir.path(), &["tag", "v-light"], &[]);
    run(
        dir.path(),
        &["tag", "-a", "v-annotated", "-m", "release notes"],
        &[],
    );

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    let refs = reader.references().expect("references");

    let light = refs
        .iter()
        .find(|entry| entry.reference == Reference::Tag(TagName::new("v-light").expect("name")))
        .expect("lightweight tag is listed");
    assert_eq!(light.target, c1);

    let annotated = refs
        .iter()
        .find(|entry| entry.reference == Reference::Tag(TagName::new("v-annotated").expect("name")))
        .expect("annotated tag is listed");
    assert_eq!(
        annotated.target, c1,
        "an annotated tag must resolve to the commit it points at, not the tag object"
    );
}

#[test]
fn stash_entries_are_read_newest_first() {
    let dir = init_repo();
    std::fs::write(dir.path().join("file.txt"), "a").expect("write file");
    run(dir.path(), &["add", "file.txt"], &[]);
    commit(dir.path(), "c1", BASE);
    std::fs::write(dir.path().join("file.txt"), "b").expect("write file");
    run(
        dir.path(),
        &["stash", "push", "--quiet", "-m", "my stash message"],
        &[],
    );

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    let stashes = reader.stashes().expect("stashes");

    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(stashes[0].message, "On main: my stash message");
}

#[test]
fn stashes_are_empty_without_a_stash() {
    let dir = init_repo();
    commit(dir.path(), "c1", BASE);

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    assert!(reader.stashes().expect("stashes").is_empty());
}

#[test]
fn submodules_report_their_checked_out_head() {
    let submodule_origin = tempfile::tempdir().expect("tempdir");
    run(
        submodule_origin.path(),
        &["init", "--quiet", "--initial-branch=main"],
        &[],
    );
    let submodule_commit = commit(submodule_origin.path(), "sub c1", BASE);

    let dir = init_repo();
    run(
        dir.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "--quiet",
            "add",
            submodule_origin.path().to_str().expect("utf8 path"),
            "vendor/lib",
        ],
        &[],
    );
    commit(dir.path(), "add submodule", BASE + 100);

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    let submodules = reader.submodules().expect("submodules");

    assert_eq!(submodules.len(), 1);
    assert_eq!(submodules[0].path, PathBuf::from("vendor/lib"));
    assert_eq!(submodules[0].head, Some(submodule_commit));
}

#[test]
fn submodules_are_empty_without_a_gitmodules_file() {
    let dir = init_repo();
    commit(dir.path(), "c1", BASE);

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    assert!(reader.submodules().expect("submodules").is_empty());
}

#[test]
fn history_scope_distinguishes_all_local_and_single() {
    let bare = tempfile::tempdir().expect("tempdir");
    run(
        bare.path(),
        &["init", "--quiet", "--bare", "--initial-branch=main"],
        &[],
    );

    let work1 = init_repo();
    let c1 = commit(work1.path(), "c1", BASE);
    run(
        work1.path(),
        &[
            "remote",
            "add",
            "origin",
            bare.path().to_str().expect("utf8 path"),
        ],
        &[],
    );
    run(work1.path(), &["push", "--quiet", "origin", "main"], &[]);
    run(
        bare.path(),
        &["symbolic-ref", "HEAD", "refs/heads/main"],
        &[],
    );

    let work2 = tempfile::tempdir().expect("tempdir");
    clone_repo(bare.path(), work2.path());
    run(work2.path(), &["checkout", "--quiet", "-b", "feature"], &[]);
    let c2 = commit(work2.path(), "c2", BASE + 100);
    run(work2.path(), &["push", "--quiet", "origin", "feature"], &[]);

    run(work1.path(), &["fetch", "--quiet", "origin"], &[]);

    let reader = GixRepositoryReader::open(work1.path()).expect("open");

    let all_ids: HashSet<_> = reader
        .history(&HistoryRequest::all())
        .expect("history")
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    assert!(
        all_ids.contains(&c2),
        "AllReferences must include commits only reachable from a remote-tracking branch"
    );
    assert!(all_ids.contains(&c1));

    let local_ids: HashSet<_> = reader
        .history(&HistoryRequest {
            scope: HistoryScope::LocalReferences,
            limit: None,
        })
        .expect("history")
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    assert!(
        !local_ids.contains(&c2),
        "LocalReferences must not include commits only reachable from a remote-tracking branch"
    );
    assert!(local_ids.contains(&c1));

    let single = reader
        .history(&HistoryRequest {
            scope: HistoryScope::Single(Reference::RemoteBranch {
                remote: RemoteName::new("origin").expect("name"),
                branch: BranchName::new("feature").expect("name"),
            }),
            limit: None,
        })
        .expect("history");
    let single_ids: Vec<_> = single.iter().map(|entry| entry.id).collect();
    assert_eq!(single_ids, vec![c2, c1]);
}

#[test]
fn history_limit_truncates_the_walk() {
    let dir = init_repo();
    commit(dir.path(), "c1", BASE);
    commit(dir.path(), "c2", BASE + 100);
    commit(dir.path(), "c3", BASE + 200);

    let reader = GixRepositoryReader::open(dir.path()).expect("open");
    let limited = reader
        .history(&HistoryRequest::limited(HistoryScope::AllReferences, 2))
        .expect("history");

    assert_eq!(limited.len(), 2);
}

#[test]
fn opening_a_non_repository_path_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");

    let result = GixRepositoryReader::open(dir.path());
    assert!(matches!(result, Err(RepositoryError::NotARepository(_))));
}

#[test]
fn opening_a_sha256_repository_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    run(
        dir.path(),
        &["init", "--quiet", "--object-format=sha256"],
        &[],
    );

    let result = GixRepositoryReader::open(dir.path());
    assert!(matches!(
        result,
        Err(RepositoryError::UnsupportedObjectFormat(_))
    ));
}

/// The test that matters most in this workstream: a history where date order and
/// topological order diverge, walked with [`domain::RepositoryReader::history`], must
/// come back in the same order as `git log --topo-order`, not `git log --date-order`.
#[test]
fn history_is_walked_in_topological_order_not_date_order() {
    let dir = init_repo();
    let path = dir.path();

    commit(path, "c1", BASE + 100);
    run(path, &["branch", "side"], &[]);
    commit(path, "c2", BASE + 200);
    commit(path, "c4", BASE + 400);
    commit(path, "c7", BASE + 700);
    run(path, &["checkout", "--quiet", "side"], &[]);
    commit(path, "c3", BASE + 300);
    commit(path, "c5", BASE + 500);
    commit(path, "c6", BASE + 600);
    git_at(
        path,
        &["merge", "--no-ff", "--quiet", "-m", "c8", "main"],
        BASE + 800,
    );

    let topo_order_via_git: Vec<domain::ObjectId> =
        run(path, &["log", "--topo-order", "--format=%H", "side"], &[])
            .lines()
            .map(|line| line.parse().expect("valid object id"))
            .collect();
    let date_order_via_git: Vec<domain::ObjectId> =
        run(path, &["log", "--date-order", "--format=%H", "side"], &[])
            .lines()
            .map(|line| line.parse().expect("valid object id"))
            .collect();
    assert_ne!(
        topo_order_via_git, date_order_via_git,
        "the fixture must make date order and topological order actually diverge"
    );

    let reader = GixRepositoryReader::open(path).expect("open");
    let history = reader
        .history(&HistoryRequest {
            scope: HistoryScope::Single(Reference::LocalBranch(
                BranchName::new("side").expect("name"),
            )),
            limit: None,
        })
        .expect("history");
    let ids: Vec<_> = history.iter().map(|entry| entry.id).collect();

    assert_eq!(
        ids, topo_order_via_git,
        "history() must walk in the same topological order as `git log --topo-order`"
    );
    assert_ne!(ids, date_order_via_git);
}
