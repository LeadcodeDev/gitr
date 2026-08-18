use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use crate::object_id::ObjectId;

/// Rejects a name Git itself would refuse to write.
///
/// This is the narrow check the sidebar and the checkout path need, not a full
/// `git check-ref-format` implementation: a name that gets past it and is still invalid
/// fails at the adapter, where Git reports the real reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidName {
    #[error("a reference name cannot be empty")]
    Empty,
    #[error("a reference name cannot contain {0:?}")]
    ForbiddenCharacter(char),
    #[error("a reference name cannot start or end with '/'")]
    LeadingOrTrailingSlash,
}

macro_rules! ref_name {
    ($name:ident, $what:literal) => {
        #[doc = concat!("A validated ", $what, " name.")]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(raw: impl Into<String>) -> Result<Self, InvalidName> {
                let raw = raw.into();
                validate(&raw)?;
                Ok(Self(raw))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

fn validate(raw: &str) -> Result<(), InvalidName> {
    if raw.is_empty() {
        return Err(InvalidName::Empty);
    }
    if raw.starts_with('/') || raw.ends_with('/') {
        return Err(InvalidName::LeadingOrTrailingSlash);
    }
    for character in raw.chars() {
        if matches!(character, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
            || character.is_control()
        {
            return Err(InvalidName::ForbiddenCharacter(character));
        }
    }
    Ok(())
}

ref_name!(BranchName, "branch");
ref_name!(RemoteName, "remote");
ref_name!(TagName, "tag");

/// A reference the sidebar can show and the history can be filtered by.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reference {
    LocalBranch(BranchName),
    RemoteBranch {
        remote: RemoteName,
        branch: BranchName,
    },
    Tag(TagName),
}

impl Reference {
    /// The full ref path, as `git` writes it.
    pub fn full_name(&self) -> String {
        match self {
            Self::LocalBranch(branch) => format!("refs/heads/{branch}"),
            Self::RemoteBranch { remote, branch } => format!("refs/remotes/{remote}/{branch}"),
            Self::Tag(tag) => format!("refs/tags/{tag}"),
        }
    }

    /// The name shown to the user — what the badges in the history render.
    pub fn short_name(&self) -> String {
        match self {
            Self::LocalBranch(branch) => branch.to_string(),
            Self::RemoteBranch { remote, branch } => format!("{remote}/{branch}"),
            Self::Tag(tag) => tag.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefEntry {
    pub reference: Reference,
    pub target: ObjectId,
    /// For a local branch, the remote branch it tracks.
    pub upstream: Option<Reference>,
}

/// Where `HEAD` points.
///
/// `Unborn` is a real state, not an error: a freshly initialised repository has a
/// branch name and no commit. Modelling it as `Option<ObjectId>` would let every caller
/// forget it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeadState {
    Unborn {
        branch: BranchName,
    },
    Attached {
        branch: BranchName,
        target: ObjectId,
    },
    Detached {
        target: ObjectId,
    },
}

impl HeadState {
    pub fn target(&self) -> Option<ObjectId> {
        match self {
            Self::Unborn { .. } => None,
            Self::Attached { target, .. } | Self::Detached { target } => Some(*target),
        }
    }

    pub fn branch(&self) -> Option<&BranchName> {
        match self {
            Self::Unborn { branch } | Self::Attached { branch, .. } => Some(branch),
            Self::Detached { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: RemoteName,
    pub fetch_url: String,
    pub push_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stash {
    /// Position in the stash reflog — the `N` of `stash@{N}`.
    pub index: usize,
    pub target: ObjectId,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submodule {
    pub path: PathBuf,
    pub url: String,
    /// `None` when the submodule is declared but not initialised.
    pub head: Option<ObjectId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_slash_separated_branch_name() {
        assert_eq!(
            BranchName::new("chantier/register-skill").unwrap().as_str(),
            "chantier/register-skill"
        );
    }

    #[test]
    fn rejects_an_empty_name() {
        assert_eq!(BranchName::new(""), Err(InvalidName::Empty));
    }

    #[test]
    fn rejects_names_git_would_refuse() {
        for (raw, expected) in [
            ("has space", InvalidName::ForbiddenCharacter(' ')),
            ("has~tilde", InvalidName::ForbiddenCharacter('~')),
            ("has:colon", InvalidName::ForbiddenCharacter(':')),
            ("/leading", InvalidName::LeadingOrTrailingSlash),
            ("trailing/", InvalidName::LeadingOrTrailingSlash),
        ] {
            assert_eq!(BranchName::new(raw), Err(expected), "for {raw:?}");
        }
    }

    #[test]
    fn renders_full_and_short_names() {
        let local = Reference::LocalBranch(BranchName::new("main").unwrap());
        assert_eq!(local.full_name(), "refs/heads/main");
        assert_eq!(local.short_name(), "main");

        let remote = Reference::RemoteBranch {
            remote: RemoteName::new("origin").unwrap(),
            branch: BranchName::new("feat/x").unwrap(),
        };
        assert_eq!(remote.full_name(), "refs/remotes/origin/feat/x");
        assert_eq!(remote.short_name(), "origin/feat/x");

        let tag = Reference::Tag(TagName::new("v1.0.0").unwrap());
        assert_eq!(tag.full_name(), "refs/tags/v1.0.0");
        assert_eq!(tag.short_name(), "v1.0.0");
    }

    #[test]
    fn an_unborn_head_has_a_branch_but_no_target() {
        let head = HeadState::Unborn {
            branch: BranchName::new("main").unwrap(),
        };
        assert_eq!(head.target(), None);
        assert!(head.branch().is_some());
    }

    #[test]
    fn a_detached_head_has_a_target_but_no_branch() {
        let head = HeadState::Detached {
            target: "d7c8a3072d02017729946165fa25f84fe35476e7".parse().unwrap(),
        };
        assert!(head.target().is_some());
        assert_eq!(head.branch(), None);
    }
}
