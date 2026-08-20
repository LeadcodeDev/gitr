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
        .delete_local_branch(repository.path(), &branch("spare"), None)
        .expect("a branch pointing at the same commit as main is merged, so -d accepts it");

    assert_eq!(local_branches(repository.path()), vec!["main".to_string()]);
}

#[test]
fn deleting_the_current_branch_switches_away_first() {
    let repository = init_repository();
    git(repository.path(), &["checkout", "-q", "-b", "feature"]);
    assert_eq!(current_branch(repository.path()), "feature");

    GitRunner::new()
        .delete_local_branch(repository.path(), &branch("feature"), Some(&branch("main")))
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
        .delete_local_branch(repository.path(), &branch("feature"), None)
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
        .delete_local_branch(repository.path(), &branch("feature"), Some(&branch("main")))
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
        .delete_local_branch(repository.path(), &branch("never-existed"), None)
        .expect_err("git cannot delete what is not there");

    match error {
        BranchError::Failed { stderr, .. } => assert!(
            stderr.contains("never-existed"),
            "the message must name the branch, got {stderr:?}"
        ),
        other => panic!("expected the generic failure to carry git's own text, got {other:?}"),
    }
}
