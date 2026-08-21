use std::path::Path;
use std::process::Command;

use domain::BranchName;
use tempfile::TempDir;
use vcs::process::{BranchError, GitRunner};

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

fn commit(repository: &Path, message: &str) {
    git(
        repository,
        &["commit", "-q", "--allow-empty", "-m", message],
    );
}

fn init_repository() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir creation cannot fail in a writable /tmp");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    commit(dir.path(), "first");
    dir
}

fn branch(name: &str) -> BranchName {
    BranchName::new(name).expect("a test branch name is valid by construction")
}

fn local_branches(repository: &Path) -> Vec<String> {
    git(
        repository,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

fn current_branch(repository: &Path) -> String {
    git(repository, &["rev-parse", "--abbrev-ref", "HEAD"])
}

#[test]
fn a_merged_branch_is_deleted() {
    let repository = init_repository();
    git(repository.path(), &["branch", "spare"]);

    GitRunner::new()
        .delete_local_branch(repository.path(), &branch("spare"), None, None)
        .expect("a branch pointing at the same commit as main is merged, so -d accepts it");

    assert_eq!(local_branches(repository.path()), vec!["main".to_string()]);
}

#[test]
fn deleting_the_current_branch_switches_away_first() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    assert_eq!(current_branch(repository.path()), "feature");

    GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            Some(&branch("main")),
            None,
        )
        .expect("switching away leaves the branch deletable");

    assert_eq!(current_branch(repository.path()), "main");
    assert_eq!(local_branches(repository.path()), vec!["main".to_string()]);
}

#[test]
fn an_unmerged_branch_is_refused_with_its_reason() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    commit(repository.path(), "work only on feature");
    git(repository.path(), &["checkout", "-q", "main"]);

    let error = GitRunner::new()
        .delete_local_branch(repository.path(), &branch("feature"), None, None)
        .expect_err("git branch -d refuses to drop commits");

    assert!(
        matches!(&error, BranchError::NotMerged),
        "expected the unmerged case to be recognised rather than reported as a bare exit \
         status, got {error:?}"
    );
    assert!(local_branches(repository.path()).contains(&"feature".to_string()));
}

#[test]
fn a_refused_switch_leaves_the_branch_alone() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(repository.path().join("tracked.txt"), "on feature\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    commit(repository.path(), "add a file only feature has");
    std::fs::write(repository.path().join("tracked.txt"), "uncommitted edit\n").unwrap();

    let error = GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            Some(&branch("main")),
            None,
        )
        .expect_err("checking out main would drop an uncommitted edit, so git refuses");

    assert!(
        matches!(&error, BranchError::SwitchRefused { target, .. } if target == "main"),
        "got {error:?}"
    );
    assert_eq!(
        current_branch(repository.path()),
        "feature",
        "a refused switch must leave the checkout where it was"
    );
    assert!(
        local_branches(repository.path()).contains(&"feature".to_string()),
        "and must not delete the branch it could not leave"
    );
}

#[test]
fn a_missing_branch_reports_git_own_reason() {
    let repository = init_repository();

    let error = GitRunner::new()
        .delete_local_branch(repository.path(), &branch("never-existed"), None, None)
        .expect_err("git cannot delete what is not there");

    match error {
        BranchError::Failed { stderr, .. } => assert!(
            stderr.contains("never-existed"),
            "the message must name the branch, got {stderr:?}"
        ),
        other => panic!("expected the generic failure to carry git's own text, got {other:?}"),
    }
}

fn write(repository: &Path, name: &str, contents: &str) {
    std::fs::write(repository.join(name), contents).unwrap();
    git(repository, &["add", name]);
}

fn squash_merge(repository: &Path, feature: &str) {
    git(repository, &["checkout", "-q", "main"]);
    git(repository, &["merge", "--squash", feature]);
    commit(repository, &format!("squash {feature}"));
}

#[test]
fn a_squash_merged_branch_is_deleted_even_though_its_commits_are_not_ancestors() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    write(repository.path(), "a.txt", "one\n");
    commit(repository.path(), "first half");
    write(repository.path(), "b.txt", "two\n");
    commit(repository.path(), "second half");
    squash_merge(repository.path(), "feature");

    assert!(
        git(repository.path(), &["branch", "--no-merged"]).contains("feature"),
        "a squash merge lands a new commit, so git's ancestry test still calls the branch \
         unmerged — which is the whole reason this case needs handling"
    );

    GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            None,
            Some(&branch("main")),
        )
        .expect("its content is in main, so nothing is lost by deleting it");

    assert_eq!(local_branches(repository.path()), vec!["main".to_string()]);
}

#[test]
fn a_branch_with_work_of_its_own_is_refused() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    write(repository.path(), "a.txt", "one\n");
    commit(repository.path(), "merged half");
    squash_merge(repository.path(), "feature");
    git(repository.path(), &["checkout", "-q", "feature"]);
    write(repository.path(), "b.txt", "kept back\n");
    commit(repository.path(), "not in main");
    git(repository.path(), &["checkout", "-q", "main"]);

    let error = GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            None,
            Some(&branch("main")),
        )
        .expect_err("part of it is in main, which is not the same as all of it");

    assert!(matches!(&error, BranchError::NotMerged), "got {error:?}");
    assert!(local_branches(repository.path()).contains(&"feature".to_string()));
}

#[test]
fn without_an_integration_branch_the_content_check_is_not_attempted() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    write(repository.path(), "a.txt", "one\n");
    commit(repository.path(), "work");
    squash_merge(repository.path(), "feature");

    let error = GitRunner::new()
        .delete_local_branch(repository.path(), &branch("feature"), None, None)
        .expect_err("with nothing to compare against, the safe refusal stands");

    assert!(matches!(&error, BranchError::NotMerged), "got {error:?}");
}

#[test]
fn the_integration_branch_is_never_absorbed_into_itself() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "side"]);
    git(repository.path(), &["checkout", "-q", "main"]);
    write(repository.path(), "only-on-main.txt", "main moved on\n");
    commit(repository.path(), "work main has and side does not");
    git(repository.path(), &["checkout", "-q", "side"]);

    let error = GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("main"),
            None,
            Some(&branch("main")),
        )
        .expect_err("comparing main against itself always matches, which must not become a -D");

    assert!(matches!(&error, BranchError::NotMerged), "got {error:?}");
    assert!(local_branches(repository.path()).contains(&"main".to_string()));
}

#[test]
fn a_branch_merged_as_several_squashes_is_still_recognised() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    write(repository.path(), "a.txt", "one\n");
    commit(repository.path(), "first landing");
    squash_merge(repository.path(), "feature");

    git(repository.path(), &["checkout", "-q", "feature"]);
    write(repository.path(), "b.txt", "two\n");
    commit(repository.path(), "second landing");
    squash_merge(repository.path(), "feature");

    GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            None,
            Some(&branch("main")),
        )
        .expect(
            "each half reached main under its own squash commit, so no single commit there \
             carries the branch's combined patch — the branch is still fully absorbed",
        );

    assert_eq!(local_branches(repository.path()), vec!["main".to_string()]);
}

#[test]
fn a_branch_whose_deletion_would_be_lost_is_refused() {
    let repository = init_repository();
    write(repository.path(), "shared.txt", "original\n");
    commit(repository.path(), "add shared file");
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::remove_file(repository.path().join("shared.txt")).unwrap();
    git(repository.path(), &["rm", "-q", "shared.txt"]);
    commit(repository.path(), "remove the shared file");
    git(repository.path(), &["checkout", "-q", "main"]);

    let error = GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            None,
            Some(&branch("main")),
        )
        .expect_err("merging it would change main, so it carries work main does not have");

    assert!(matches!(&error, BranchError::NotMerged), "got {error:?}");
    assert!(local_branches(repository.path()).contains(&"feature".to_string()));
}

#[test]
fn a_branch_is_recognised_even_when_replaying_it_would_now_conflict() {
    let repository = init_repository();
    write(repository.path(), "notes.md", "first line\n");
    commit(repository.path(), "add notes");
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    write(
        repository.path(),
        "notes.md",
        "first line\nfrom the branch\n",
    );
    commit(repository.path(), "extend the notes");
    squash_merge(repository.path(), "feature");
    write(
        repository.path(),
        "notes.md",
        "first line\nrewritten on main afterwards\n",
    );
    commit(repository.path(), "main edits the same lines later");

    assert!(
        GitRunner::new()
            .is_absorbed_by(repository.path(), &branch("feature"), &branch("main"),)
            .unwrap(),
        "the branch landed, then main rewrote the same lines, so replaying it three-way \
         conflicts — its patch is still in main's history and nothing is lost by deleting it"
    );

    GitRunner::new()
        .delete_local_branch(
            repository.path(),
            &branch("feature"),
            None,
            Some(&branch("main")),
        )
        .expect("and so the deletion goes through");
}
