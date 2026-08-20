use std::path::Path;

use domain::BranchName;

use super::runner::{GitProcessError, GitRunner};

#[derive(Debug, thiserror::Error)]
pub enum BranchError {
    #[error("it has commits that are in no other branch")]
    NotMerged,
    #[error("cannot leave the current branch for {target}: {stderr}")]
    SwitchRefused { target: String, stderr: String },
    #[error("git exited with status {status}: {stderr}")]
    Failed { status: i32, stderr: String },
    #[error("could not run git: {0}")]
    Unavailable(String),
}

impl GitRunner {
    pub fn delete_local_branch(
        &self,
        repository: &Path,
        branch: &BranchName,
        switch_to: Option<&BranchName>,
        integration: Option<&BranchName>,
    ) -> Result<(), BranchError> {
        if let Some(target) = switch_to {
            self.run(repository, &["checkout", target.as_str()])
                .map_err(|error| match error {
                    GitProcessError::Spawn(source) => BranchError::Unavailable(source.to_string()),
                    GitProcessError::Failed { stderr, .. } => BranchError::SwitchRefused {
                        target: target.to_string(),
                        stderr: summarise(&stderr),
                    },
                })?;
        }

        match self.run(repository, &["branch", "-d", branch.as_str()]) {
            Ok(_) => Ok(()),
            Err(error) => match classify(error) {
                BranchError::NotMerged => self.delete_if_squash_merged(
                    repository,
                    branch,
                    integration.ok_or(BranchError::NotMerged)?,
                ),
                other => Err(other),
            },
        }
    }

    fn delete_if_squash_merged(
        &self,
        repository: &Path,
        branch: &BranchName,
        integration: &BranchName,
    ) -> Result<(), BranchError> {
        if !self.is_squash_merged(repository, branch, integration)? {
            return Err(BranchError::NotMerged);
        }
        self.run(repository, &["branch", "-D", branch.as_str()])
            .map(|_| ())
            .map_err(classify)
    }

    pub fn is_squash_merged(
        &self,
        repository: &Path,
        branch: &BranchName,
        integration: &BranchName,
    ) -> Result<bool, BranchError> {
        if branch == integration {
            return Ok(false);
        }
        let base = self.capture(
            repository,
            &["merge-base", integration.as_str(), branch.as_str()],
        )?;
        let tree = self.capture(repository, &["rev-parse", &format!("{branch}^{{tree}}")])?;
        let base_tree = self.capture(repository, &["rev-parse", &format!("{base}^{{tree}}")])?;
        if tree == base_tree {
            return Ok(true);
        }

        let replay = self.capture(
            repository,
            &[
                "commit-tree",
                &tree,
                "-p",
                &base,
                "-m",
                "gitr squash-merge probe",
            ],
        )?;
        let cherry = self.capture(repository, &["cherry", integration.as_str(), &replay])?;

        Ok(cherry.lines().all(|line| line.starts_with('-')) && !cherry.trim().is_empty())
    }

    fn capture(&self, repository: &Path, args: &[&str]) -> Result<String, BranchError> {
        self.run(repository, args)
            .map(|output| output.stdout.trim().to_string())
            .map_err(classify)
    }
}

fn classify(error: GitProcessError) -> BranchError {
    match error {
        GitProcessError::Spawn(source) => BranchError::Unavailable(source.to_string()),
        GitProcessError::Failed { status, stderr } => {
            if stderr.to_lowercase().contains("not fully merged") {
                BranchError::NotMerged
            } else {
                BranchError::Failed {
                    status,
                    stderr: summarise(&stderr),
                }
            }
        }
    }
}

fn summarise(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("git reported no reason")
        .to_string()
}
