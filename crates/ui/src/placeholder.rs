//! Hand-made repository data the shell renders until `crates/vcs` has a working adapter.
//!
//! Every type here is `domain`'s own — no parallel model is invented. The integration
//! point for the real reader is [`placeholder_repositories`]: once `vcs` exists, that is
//! the one call site [`crate::workspace::Workspace::new`] replaces with a
//! `domain::RepositoryReader` call run on `cx.background_executor()`. Nothing downstream
//! needs to change shape, only where the data comes from.

use domain::{
    BranchName, FilePatch, FileStatus, HeadState, ObjectId, RefEntry, Reference, RemoteName, Stash,
    TagName,
};
use std::path::PathBuf;

/// One repository as the sidebar, title bar and status bar need to see it.
pub struct PlaceholderRepository {
    pub name: String,
    pub head: HeadState,
    pub references: Vec<RefEntry>,
    pub stashes: Vec<Stash>,
    pub working_tree_changes: Vec<FilePatch>,
    pub commit_count: usize,
}

impl PlaceholderRepository {
    pub fn local_branches(&self) -> impl Iterator<Item = &Reference> {
        self.references
            .iter()
            .map(|entry| &entry.reference)
            .filter(|reference| matches!(reference, Reference::LocalBranch(_)))
    }

    pub fn remote_branches(&self) -> impl Iterator<Item = &Reference> {
        self.references
            .iter()
            .map(|entry| &entry.reference)
            .filter(|reference| matches!(reference, Reference::RemoteBranch { .. }))
    }

    pub fn tags(&self) -> impl Iterator<Item = &Reference> {
        self.references
            .iter()
            .map(|entry| &entry.reference)
            .filter(|reference| matches!(reference, Reference::Tag(_)))
    }
}

/// A [`BranchName`] built from a literal known at the call site below to be valid.
///
/// A failure here is a typo in this file, not a runtime condition — the strings are all
/// hardcoded a few lines down, never user or repository input.
fn branch(name: &str) -> BranchName {
    BranchName::new(name).expect("hardcoded placeholder branch name must be valid")
}

fn remote_name(name: &str) -> RemoteName {
    RemoteName::new(name).expect("hardcoded placeholder remote name must be valid")
}

fn tag_name(name: &str) -> TagName {
    TagName::new(name).expect("hardcoded placeholder tag name must be valid")
}

fn local(name: &str) -> Reference {
    Reference::LocalBranch(branch(name))
}

fn remote_branch(remote: &str, name: &str) -> Reference {
    Reference::RemoteBranch {
        remote: remote_name(remote),
        branch: branch(name),
    }
}

fn tag(name: &str) -> Reference {
    Reference::Tag(tag_name(name))
}

/// A distinct, deterministic [`ObjectId`] per `seed`, for placeholder data only.
///
/// Not a hash of anything — just a byte pattern that reads recognisably in a debugger
/// and never collides across the handful of placeholder commits below.
fn object_id(seed: u8) -> ObjectId {
    let mut bytes = [0u8; 20];
    bytes[0] = seed;
    ObjectId::from_bytes(bytes)
}

fn ref_entry(reference: Reference, target: ObjectId, upstream: Option<Reference>) -> RefEntry {
    RefEntry {
        reference,
        target,
        upstream,
    }
}

fn gitr_repository() -> PlaceholderRepository {
    let main = local("main");
    let m1 = local("chantier/m1-read-and-visualise");
    let register_skill = local("chantier/register-skill");
    let status_bar = local("feature/status-bar");
    let origin_main = remote_branch("origin", "main");
    let origin_m1 = remote_branch("origin", "chantier/m1-read-and-visualise");

    PlaceholderRepository {
        name: "gitr".to_string(),
        head: HeadState::Attached {
            branch: branch("chantier/m1-read-and-visualise"),
            target: object_id(0x10),
        },
        references: vec![
            ref_entry(main, object_id(0x01), Some(origin_main.clone())),
            ref_entry(m1, object_id(0x10), Some(origin_m1.clone())),
            ref_entry(register_skill, object_id(0x11), None),
            ref_entry(status_bar, object_id(0x12), None),
            ref_entry(origin_main, object_id(0x01), None),
            ref_entry(origin_m1, object_id(0x10), None),
            ref_entry(tag("v0.1.0"), object_id(0x02), None),
            ref_entry(tag("v0.2.0"), object_id(0x03), None),
        ],
        stashes: vec![Stash {
            index: 0,
            target: object_id(0x20),
            message: "WIP on chantier/m1-read-and-visualise: sidebar layout".to_string(),
        }],
        working_tree_changes: vec![
            FilePatch {
                old_path: None,
                new_path: Some(PathBuf::from("crates/ui/src/workspace.rs")),
                status: FileStatus::Added,
                is_binary: false,
                hunks: Vec::new(),
            },
            FilePatch {
                old_path: Some(PathBuf::from("crates/ui/src/lib.rs")),
                new_path: Some(PathBuf::from("crates/ui/src/lib.rs")),
                status: FileStatus::Modified,
                is_binary: false,
                hunks: Vec::new(),
            },
        ],
        commit_count: 128,
    }
}

fn scratch_repository() -> PlaceholderRepository {
    let main = local("main");

    PlaceholderRepository {
        name: "scratch".to_string(),
        head: HeadState::Attached {
            branch: branch("main"),
            target: object_id(0x30),
        },
        references: vec![ref_entry(main, object_id(0x30), None)],
        stashes: Vec::new(),
        working_tree_changes: Vec::new(),
        commit_count: 4,
    }
}

/// One window, several repositories: this is the whole list the sidebar's repository
/// selector switches between until a real repository can be opened.
pub fn placeholder_repositories() -> Vec<PlaceholderRepository> {
    vec![gitr_repository(), scratch_repository()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_repository_reference_sorts_into_exactly_one_kind() {
        for repository in placeholder_repositories() {
            let local = repository.local_branches().count();
            let remote = repository.remote_branches().count();
            let tags = repository.tags().count();
            assert_eq!(local + remote + tags, repository.references.len());
        }
    }

    #[test]
    fn gitr_repository_head_matches_a_local_branch_it_lists() {
        let repository = gitr_repository();
        let HeadState::Attached { branch, .. } = &repository.head else {
            panic!("placeholder HEAD must be attached");
        };
        assert!(
            repository
                .local_branches()
                .any(|reference| reference == &Reference::LocalBranch(branch.clone()))
        );
    }
}
