use std::num::NonZeroU8;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// One facet of a repository that a view may need to reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Aspect {
    /// `HEAD` moved — checkout, commit, reset, or a step of a rebase.
    Head,
    /// A branch, tag or remote-tracking reference changed — fetch, push, branch edit.
    References,
    /// The staging area changed.
    Index,
    /// Tracked files in the working tree changed.
    WorkingTree,
}

impl Aspect {
    const fn bit(self) -> u8 {
        match self {
            Self::Head => 1,
            Self::References => 1 << 1,
            Self::Index => 1 << 2,
            Self::WorkingTree => 1 << 3,
        }
    }
}

/// A non-empty set of [`Aspect`]s that changed together.
///
/// A set rather than a single aspect because one user action touches several at once:
/// `git checkout` moves `HEAD`, rewrites the index and rewrites the working tree. Sending
/// three separate notifications would make a view reload three times for one checkout.
///
/// Emptiness is unrepresentable, so "nothing changed" cannot be sent by mistake — the
/// watcher either has something to report or stays silent.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RepositoryChange(NonZeroU8);

impl RepositoryChange {
    pub const fn only(aspect: Aspect) -> Self {
        match NonZeroU8::new(aspect.bit()) {
            Some(bits) => Self(bits),
            None => unreachable!(),
        }
    }

    /// Every aspect at once — what a caller reloads after losing track of events.
    pub const fn everything() -> Self {
        match NonZeroU8::new(0b1111) {
            Some(bits) => Self(bits),
            None => unreachable!(),
        }
    }

    pub const fn with(self, aspect: Aspect) -> Self {
        match NonZeroU8::new(self.0.get() | aspect.bit()) {
            Some(bits) => Self(bits),
            None => unreachable!(),
        }
    }

    pub const fn merge(self, other: Self) -> Self {
        match NonZeroU8::new(self.0.get() | other.0.get()) {
            Some(bits) => Self(bits),
            None => unreachable!(),
        }
    }

    pub const fn contains(self, aspect: Aspect) -> bool {
        self.0.get() & aspect.bit() != 0
    }

    pub fn aspects(self) -> impl Iterator<Item = Aspect> {
        [
            Aspect::Head,
            Aspect::References,
            Aspect::Index,
            Aspect::WorkingTree,
        ]
        .into_iter()
        .filter(move |aspect| self.contains(*aspect))
    }
}

impl std::fmt::Debug for RepositoryChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_set()
            .entries(self.aspects().map(|aspect| format!("{aspect:?}")))
            .finish()
    }
}

/// Why a repository could not be watched.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("{0} is not a Git repository")]
    NotARepository(PathBuf),
    #[error("{0} does not exist or cannot be read")]
    Unreadable(PathBuf),
    #[error("the platform refused to watch {path}: {source}")]
    Rejected {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Reports changes made to a repository by anything other than gitr.
///
/// The user runs `git checkout`, `git rebase` or `git fetch` in a terminal while gitr has
/// the same repository open. Without this, every view is stale until something forces a
/// reload — the failing behaviour GitX papers over with a refresh button.
///
/// Implementations must satisfy three properties that are not visible in the signature:
///
/// - **Debounced.** A single `git fetch` rewrites many files under `refs/`. Events are
///   coalesced into one [`RepositoryChange`] rather than one per file.
/// - **Indirection-aware.** `.git` is a file rather than a directory in a linked worktree
///   and in a submodule. The real Git directory must be resolved before watching, or the
///   watch silently observes nothing.
/// - **Loss-tolerant.** If the platform drops events or the queue overflows, send
///   [`RepositoryChange::everything`] rather than going quiet. A watcher that silently
///   stops is worse than none, because the view still looks live.
///
/// Once gitr performs its own mutations, those will fire this watcher too. Suppressing
/// that echo belongs to the caller that initiated the write, not here: the watcher cannot
/// tell who moved `HEAD`.
pub trait RepositoryWatcher {
    /// Stops watching when dropped.
    type Guard: Send;

    /// Begins watching `repository`, sending coalesced changes to `changes`.
    ///
    /// The send end is closed when the guard is dropped, so a receiver loop terminates on
    /// a disconnected channel rather than needing its own shutdown signal.
    fn watch(
        &self,
        repository: &Path,
        changes: Sender<RepositoryChange>,
    ) -> Result<Self::Guard, WatchError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_aspect_contains_only_itself() {
        let change = RepositoryChange::only(Aspect::Head);
        assert!(change.contains(Aspect::Head));
        assert!(!change.contains(Aspect::References));
        assert!(!change.contains(Aspect::Index));
        assert!(!change.contains(Aspect::WorkingTree));
    }

    #[test]
    fn everything_contains_every_aspect() {
        let change = RepositoryChange::everything();
        assert_eq!(change.aspects().count(), 4);
    }

    #[test]
    fn a_checkout_shaped_change_carries_three_aspects() {
        let change = RepositoryChange::only(Aspect::Head)
            .with(Aspect::Index)
            .with(Aspect::WorkingTree);
        assert_eq!(
            change.aspects().collect::<Vec<_>>(),
            vec![Aspect::Head, Aspect::Index, Aspect::WorkingTree]
        );
    }

    #[test]
    fn merging_is_a_union() {
        let head = RepositoryChange::only(Aspect::Head);
        let references = RepositoryChange::only(Aspect::References);
        let merged = head.merge(references);
        assert!(merged.contains(Aspect::Head));
        assert!(merged.contains(Aspect::References));
        assert_eq!(merged.aspects().count(), 2);
    }

    #[test]
    fn merging_a_set_with_itself_changes_nothing() {
        let change = RepositoryChange::only(Aspect::Index);
        assert_eq!(change.merge(change), change);
    }

    #[test]
    fn aspects_are_yielded_in_a_stable_order() {
        let change = RepositoryChange::only(Aspect::WorkingTree).with(Aspect::Head);
        assert_eq!(
            change.aspects().collect::<Vec<_>>(),
            vec![Aspect::Head, Aspect::WorkingTree]
        );
    }

    #[test]
    fn a_change_is_one_byte_and_copy() {
        assert_eq!(size_of::<RepositoryChange>(), 1);
        assert_eq!(size_of::<Option<RepositoryChange>>(), 1);
    }
}
