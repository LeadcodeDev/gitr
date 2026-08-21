use domain::BranchName;
use gpui::{ParentElement as _, SharedString, Styled as _, WeakEntity};
use gpui_component::{
    Icon, IconName, h_flex,
    menu::{PopupMenu, PopupMenuItem},
};

use crate::workspace::Workspace;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deletion {
    pub head: Option<BranchName>,
    pub fallback: Option<BranchName>,
}

impl Deletion {
    pub fn switch_to(&self, branch: &BranchName) -> Option<Option<BranchName>> {
        if self.head.as_ref() != Some(branch) {
            return Some(None);
        }
        match &self.fallback {
            Some(fallback) if fallback != branch => Some(Some(fallback.clone())),
            _ => None,
        }
    }
}

pub fn branch_menu(
    menu: PopupMenu,
    branch: &BranchName,
    switch_to: Option<&BranchName>,
    workspace: &WeakEntity<Workspace>,
) -> PopupMenu {
    menu.label(MENU_TITLE)
        .item(delete_menu_item(branch, switch_to, workspace))
}

const MENU_TITLE: &str = "Actions";

fn delete_menu_item(
    branch: &BranchName,
    switch_to: Option<&BranchName>,
    workspace: &WeakEntity<Workspace>,
) -> PopupMenuItem {
    let label: SharedString = match switch_to {
        Some(fallback) => format!("Delete branch and switch to {fallback}").into(),
        None => "Delete branch".into(),
    };
    let workspace = workspace.clone();
    let branch = branch.clone();

    PopupMenuItem::element(move |_, _| {
        h_flex()
            .items_center()
            .gap_2()
            .child(Icon::new(IconName::Delete).size(ICON_SIZE))
            .child(label.clone())
    })
    .on_click(move |_, window, cx| {
        let _ = workspace.update(cx, |workspace, cx| {
            workspace.delete_local_branch(branch.clone(), window, cx);
        });
    })
}

const ICON_SIZE: gpui::Pixels = gpui::px(16.);

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str) -> BranchName {
        BranchName::new(name).unwrap()
    }

    fn deletion(head: Option<&str>, fallback: Option<&str>) -> Deletion {
        Deletion {
            head: head.map(branch),
            fallback: fallback.map(branch),
        }
    }

    #[test]
    fn a_branch_that_is_not_checked_out_is_deleted_where_it_stands() {
        assert_eq!(
            deletion(Some("main"), Some("main")).switch_to(&branch("feature")),
            Some(None),
            "nothing to switch away from, so the outer Some means deletable and the inner \
             None means stay put"
        );
    }

    #[test]
    fn the_checked_out_branch_is_deleted_after_switching_to_the_fallback() {
        assert_eq!(
            deletion(Some("feature"), Some("main")).switch_to(&branch("feature")),
            Some(Some(branch("main")))
        );
    }

    #[test]
    fn the_checked_out_branch_is_not_deletable_without_a_fallback() {
        assert_eq!(
            deletion(Some("feature"), None).switch_to(&branch("feature")),
            None,
            "offering it would put the repository on no branch at all"
        );
    }

    #[test]
    fn the_fallback_branch_is_not_deletable_while_standing_on_it() {
        assert_eq!(
            deletion(Some("main"), Some("main")).switch_to(&branch("main")),
            None,
            "switching to the branch being deleted is not a way out of it"
        );
    }

    #[test]
    fn an_unknown_head_leaves_every_branch_deletable() {
        assert_eq!(
            deletion(None, Some("main")).switch_to(&branch("feature")),
            Some(None),
            "a detached HEAD is on no branch, so no branch is the one being stood on"
        );
    }
}
