use std::path::Path;

use domain::Aspect;

/// Where a changed path was observed, before it is mapped to an [`Aspect`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Location<'a> {
    /// `0` is the path with the Git directory prefix stripped off.
    GitDir(&'a Path),
    WorkingTree,
}

/// Maps an observed path to the [`Aspect`] a caller should reload for it.
///
/// `None` means the path is transient (a `*.lock` file), redundant (the reflog, which
/// changes exactly when `HEAD` or a ref does), voluminous by design (`objects/`), or simply
/// not one this watcher has an opinion about (`hooks/`, `config`, `description`). All three
/// are deliberately silent rather than escalated: only paths git actually treats as state
/// worth acting on carry a signal here.
pub(crate) fn classify(location: Location<'_>) -> Option<Aspect> {
    match location {
        Location::WorkingTree => Some(Aspect::WorkingTree),
        Location::GitDir(relative) => classify_git_dir_path(relative),
    }
}

fn classify_git_dir_path(relative: &Path) -> Option<Aspect> {
    if has_lock_suffix(relative) {
        return None;
    }

    let first = relative.components().next()?.as_os_str().to_str()?;

    match first {
        "HEAD" | "ORIG_HEAD" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD" => {
            Some(Aspect::Head)
        }
        "rebase-merge" | "rebase-apply" => Some(Aspect::Head),
        "index" => Some(Aspect::Index),
        "refs" | "packed-refs" | "FETCH_HEAD" => Some(Aspect::References),
        _ => None,
    }
}

fn has_lock_suffix(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_dir(relative: &str) -> Option<Aspect> {
        classify(Location::GitDir(Path::new(relative)))
    }

    #[test]
    fn head_is_head() {
        assert_eq!(git_dir("HEAD"), Some(Aspect::Head));
    }

    #[test]
    fn orig_head_is_head() {
        assert_eq!(git_dir("ORIG_HEAD"), Some(Aspect::Head));
    }

    #[test]
    fn merge_head_is_head() {
        assert_eq!(git_dir("MERGE_HEAD"), Some(Aspect::Head));
    }

    #[test]
    fn rebase_head_is_head() {
        assert_eq!(git_dir("REBASE_HEAD"), Some(Aspect::Head));
    }

    #[test]
    fn cherry_pick_head_is_head() {
        assert_eq!(git_dir("CHERRY_PICK_HEAD"), Some(Aspect::Head));
    }

    #[test]
    fn a_file_inside_rebase_merge_is_head() {
        assert_eq!(git_dir("rebase-merge/msgnum"), Some(Aspect::Head));
    }

    #[test]
    fn a_file_inside_rebase_apply_is_head() {
        assert_eq!(git_dir("rebase-apply/next"), Some(Aspect::Head));
    }

    #[test]
    fn index_is_index() {
        assert_eq!(git_dir("index"), Some(Aspect::Index));
    }

    #[test]
    fn packed_refs_is_references() {
        assert_eq!(git_dir("packed-refs"), Some(Aspect::References));
    }

    #[test]
    fn fetch_head_is_references() {
        assert_eq!(git_dir("FETCH_HEAD"), Some(Aspect::References));
    }

    #[test]
    fn a_top_level_branch_ref_is_references() {
        assert_eq!(git_dir("refs/heads/main"), Some(Aspect::References));
    }

    #[test]
    fn a_deeply_nested_ref_is_references() {
        assert_eq!(git_dir("refs/heads/chantier/m1"), Some(Aspect::References));
    }

    #[test]
    fn a_lock_file_is_ignored() {
        assert_eq!(git_dir("index.lock"), None);
    }

    #[test]
    fn a_ref_lock_file_is_ignored() {
        assert_eq!(git_dir("refs/heads/main.lock"), None);
    }

    #[test]
    fn a_loose_object_is_ignored() {
        assert_eq!(
            git_dir("objects/4b/825dc642cb6eb9a060e54bf8d69288fbee4904"),
            None
        );
    }

    #[test]
    fn the_reflog_is_ignored() {
        assert_eq!(git_dir("logs/HEAD"), None);
        assert_eq!(git_dir("logs/refs/heads/main"), None);
    }

    #[test]
    fn an_unrecognised_path_is_ignored() {
        assert_eq!(git_dir("hooks/pre-commit"), None);
        assert_eq!(git_dir("config"), None);
        assert_eq!(git_dir("description"), None);
    }

    #[test]
    fn a_working_tree_path_is_working_tree() {
        assert_eq!(classify(Location::WorkingTree), Some(Aspect::WorkingTree));
    }
}
