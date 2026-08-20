//! The repository sidebar: repository selector, and the `Working` / `Branches` /
//! `Remotes` / `Tags` / `Stashes` disclosure sections beneath it.
//!
//! Rendered as plain window chrome next to [`gpui_component::dock::DockArea`], not as
//! one of its docks — the repository list and reference tree are permanent navigation,
//! not a tool window a user drags, closes or expects the dock layout to remember.
//!
//! `Working` and `Stashes` render with no children: [`crate::repository::RepositoryState`]
//! does not load working-tree changes or stashes yet, so there is nothing real to show.

pub mod branch_tree;
pub(crate) mod selector;
pub mod tree;

use domain::{BranchName, HeadState, Reference};
use gpui::{Context, WeakEntity};
use gpui_component::{IconName, menu::PopupMenuItem, sidebar::Sidebar};

use crate::{
    project::ProjectList,
    repository::{LoadState, ReferenceIndex},
    workspace::Workspace,
};

use selector::SelectorInputs;
use tree::SidebarTreeItem;

pub(crate) fn render(
    references: &LoadState<ReferenceIndex>,
    head: &LoadState<HeadState>,
    projects: &ProjectList,
    selector: SelectorInputs<'_>,
    collapsed: bool,
    cx: &mut Context<Workspace>,
) -> Sidebar<SidebarTreeItem> {
    let active_branch = head
        .ready()
        .and_then(HeadState::branch)
        .map(|branch| Reference::LocalBranch(branch.clone()));
    let index = references.ready();
    let workspace = cx.entity().downgrade();
    let deletion = Deletion {
        head: head.ready().and_then(HeadState::branch).cloned(),
        fallback: index.and_then(ReferenceIndex::fallback_branch),
    };

    let items = vec![
        working_item(),
        branches_item(index, active_branch.as_ref(), &workspace, &deletion),
        remotes_item(index, active_branch.as_ref(), &workspace),
        tags_item(index, &workspace),
        stashes_item(),
    ];

    Sidebar::new("repository-sidebar")
        .collapsed(collapsed)
        .header(selector::popover(projects, selector, collapsed, cx))
        .children(items)
}

fn working_item() -> SidebarTreeItem {
    SidebarTreeItem::new("Working").icon(IconName::FolderOpen)
}

#[derive(Clone)]
struct Deletion {
    head: Option<BranchName>,
    fallback: Option<BranchName>,
}

impl Deletion {
    fn switch_to(&self, branch: &BranchName) -> Option<Option<BranchName>> {
        if self.head.as_ref() != Some(branch) {
            return Some(None);
        }
        match &self.fallback {
            Some(fallback) if fallback != branch => Some(Some(fallback.clone())),
            _ => None,
        }
    }
}

fn delete_menu_item(
    branch: &BranchName,
    switch_to: Option<&BranchName>,
    workspace: &WeakEntity<Workspace>,
) -> PopupMenuItem {
    let label = match switch_to {
        Some(fallback) => format!("Delete branch and switch to {fallback}"),
        None => "Delete branch".to_string(),
    };
    let workspace = workspace.clone();
    let branch = branch.clone();

    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = workspace.update(cx, |workspace, cx| {
            workspace.delete_local_branch(branch.clone(), window, cx);
        });
    })
}

fn tree_item(
    node: &branch_tree::RefTreeNode,
    active: Option<&Reference>,
    workspace: &WeakEntity<Workspace>,
    deletion: Option<&Deletion>,
) -> SidebarTreeItem {
    let is_active = node
        .reference
        .as_ref()
        .zip(active)
        .is_some_and(|(reference, active)| reference == active);

    let mut item = SidebarTreeItem::new(node.segment.clone()).active(is_active);

    if let Some(reference) = node.reference.clone() {
        let workspace = workspace.clone();
        item = item.on_click(move |_, _, cx| {
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.filter_by_reference(reference.clone(), cx);
            });
        });
    }

    if let (Some(Reference::LocalBranch(branch)), Some(deletion)) =
        (node.reference.as_ref(), deletion)
        && let Some(switch_to) = deletion.switch_to(branch)
    {
        let branch = branch.clone();
        let workspace = workspace.clone();
        item = item.context_menu(move |menu, _, _| {
            menu.item(delete_menu_item(&branch, switch_to.as_ref(), &workspace))
        });
    }

    if !node.children.is_empty() {
        item = item.default_open(true).children(
            node.children
                .iter()
                .map(|child| tree_item(child, active, workspace, deletion)),
        );
    }

    item
}

fn branches_item(
    index: Option<&ReferenceIndex>,
    active: Option<&Reference>,
    workspace: &WeakEntity<Workspace>,
    deletion: &Deletion,
) -> SidebarTreeItem {
    let references = index.into_iter().flat_map(|index| {
        index
            .local_branches
            .iter()
            .map(|entry| entry.reference.clone())
    });
    let tree = branch_tree::group_by_path(references);
    let mut item = SidebarTreeItem::new("Branches").icon(IconName::Network);

    if !tree.is_empty() {
        item = item.default_open(true).children(
            tree.iter()
                .map(|node| tree_item(node, active, workspace, Some(deletion))),
        );
    }

    item
}

fn remotes_item(
    index: Option<&ReferenceIndex>,
    active: Option<&Reference>,
    workspace: &WeakEntity<Workspace>,
) -> SidebarTreeItem {
    let references = index.into_iter().flat_map(|index| {
        index
            .remote_branches
            .iter()
            .map(|entry| entry.reference.clone())
    });
    let tree = branch_tree::group_by_path(references);
    let mut item = SidebarTreeItem::new("Remotes").icon(IconName::Globe);

    if !tree.is_empty() {
        item = item.children(
            tree.iter()
                .map(|node| tree_item(node, active, workspace, None)),
        );
    }

    item
}

fn tags_item(index: Option<&ReferenceIndex>, workspace: &WeakEntity<Workspace>) -> SidebarTreeItem {
    let references = index
        .into_iter()
        .flat_map(|index| index.tags.iter().map(|entry| entry.reference.clone()));
    let tree = branch_tree::group_by_path(references);
    let mut item = SidebarTreeItem::new("Tags");

    if !tree.is_empty() {
        item = item.children(
            tree.iter()
                .map(|node| tree_item(node, None, workspace, None)),
        );
    }

    item
}

fn stashes_item() -> SidebarTreeItem {
    SidebarTreeItem::new("Stashes")
}
