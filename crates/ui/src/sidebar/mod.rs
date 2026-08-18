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

use domain::{HeadState, Reference};
use gpui::{App, Context, IntoElement, ParentElement as _, Styled as _, WeakEntity, div};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, StyledExt as _,
    menu::{DropdownMenu as _, PopupMenuItem},
    sidebar::{Sidebar, SidebarHeader, SidebarMenu, SidebarMenuItem},
};

use crate::{
    repository::{LoadState, ReferenceIndex},
    workspace::Workspace,
};

pub(crate) fn render(
    references: &LoadState<ReferenceIndex>,
    head: &LoadState<HeadState>,
    repository_name: &str,
    collapsed: bool,
    cx: &mut Context<Workspace>,
) -> Sidebar<SidebarMenu> {
    let active_branch = head
        .ready()
        .and_then(HeadState::branch)
        .map(|branch| Reference::LocalBranch(branch.clone()));
    let index = references.ready();
    let workspace = cx.entity().downgrade();

    let items = vec![
        working_item(),
        branches_item(index, active_branch.as_ref(), &workspace),
        remotes_item(index, active_branch.as_ref(), &workspace),
        tags_item(index, &workspace),
        stashes_item(),
    ];

    Sidebar::new("repository-sidebar")
        .collapsed(collapsed)
        .header(repository_header(cx, repository_name, collapsed))
        .child(SidebarMenu::new().children(items))
}

/// The repository selector, keeping the shape a future multi-repository window will
/// need. It lists exactly the one open repository, always checked: there is nothing
/// else to switch to yet, so no `on_click` pretends otherwise.
fn repository_header(cx: &App, repository_name: &str, collapsed: bool) -> impl IntoElement {
    let name = repository_name.to_string();

    let mut header = SidebarHeader::new().child(
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_8()
            .flex_shrink_0()
            .rounded(cx.theme().radius)
            .bg(cx.theme().accent)
            .text_color(cx.theme().accent_foreground)
            .child(Icon::new(IconName::FolderOpen)),
    );

    if !collapsed {
        header = header
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_sm()
                    .font_medium()
                    .child(name.clone()),
            )
            .child(Icon::new(IconName::ChevronsUpDown).size_4().flex_shrink_0());
    }

    header
        .dropdown_menu(move |menu, _, _| menu.item(PopupMenuItem::new(name.clone()).checked(true)))
}

fn working_item() -> SidebarMenuItem {
    SidebarMenuItem::new("Working").icon(IconName::FolderOpen)
}

fn tree_item(
    node: &branch_tree::RefTreeNode,
    active: Option<&Reference>,
    workspace: &WeakEntity<Workspace>,
) -> SidebarMenuItem {
    let is_active = node
        .reference
        .as_ref()
        .zip(active)
        .is_some_and(|(reference, active)| reference == active);

    let mut item = SidebarMenuItem::new(node.segment.clone()).active(is_active);

    if let Some(reference) = node.reference.clone() {
        let workspace = workspace.clone();
        item = item.on_click(move |_, _, cx| {
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.filter_by_reference(reference.clone(), cx);
            });
        });
    }

    if !node.children.is_empty() {
        item = item.default_open(true).children(
            node.children
                .iter()
                .map(|child| tree_item(child, active, workspace)),
        );
    }

    item
}

fn branches_item(
    index: Option<&ReferenceIndex>,
    active: Option<&Reference>,
    workspace: &WeakEntity<Workspace>,
) -> SidebarMenuItem {
    let references = index.into_iter().flat_map(|index| {
        index
            .local_branches
            .iter()
            .map(|entry| entry.reference.clone())
    });
    let tree = branch_tree::group_by_path(references);
    let mut item = SidebarMenuItem::new("Branches").icon(IconName::Network);

    if !tree.is_empty() {
        item = item
            .default_open(true)
            .children(tree.iter().map(|node| tree_item(node, active, workspace)));
    }

    item
}

fn remotes_item(
    index: Option<&ReferenceIndex>,
    active: Option<&Reference>,
    workspace: &WeakEntity<Workspace>,
) -> SidebarMenuItem {
    let references = index.into_iter().flat_map(|index| {
        index
            .remote_branches
            .iter()
            .map(|entry| entry.reference.clone())
    });
    let tree = branch_tree::group_by_path(references);
    let mut item = SidebarMenuItem::new("Remotes").icon(IconName::Globe);

    if !tree.is_empty() {
        item = item.children(tree.iter().map(|node| tree_item(node, active, workspace)));
    }

    item
}

fn tags_item(index: Option<&ReferenceIndex>, workspace: &WeakEntity<Workspace>) -> SidebarMenuItem {
    let references = index
        .into_iter()
        .flat_map(|index| index.tags.iter().map(|entry| entry.reference.clone()));
    let tree = branch_tree::group_by_path(references);
    let mut item = SidebarMenuItem::new("Tags");

    if !tree.is_empty() {
        item = item.children(tree.iter().map(|node| tree_item(node, None, workspace)));
    }

    item
}

fn stashes_item() -> SidebarMenuItem {
    SidebarMenuItem::new("Stashes")
}
