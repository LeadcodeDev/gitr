//! The repository sidebar: repository selector, and the `Working` / `Branches` /
//! `Remotes` / `Tags` / `Stashes` disclosure sections beneath it.
//!
//! Rendered as plain window chrome next to [`gpui_component::dock::DockArea`], not as
//! one of its docks — the repository list and reference tree are permanent navigation,
//! not a tool window a user drags, closes or expects the dock layout to remember.

pub mod branch_tree;

use domain::{FilePatch, FileStatus, Reference};
use gpui::{App, Context, IntoElement, ParentElement as _, Styled as _, WeakEntity, div};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, StyledExt as _,
    menu::{DropdownMenu as _, PopupMenuItem},
    sidebar::{Sidebar, SidebarHeader, SidebarMenu, SidebarMenuItem},
};

use crate::{placeholder::PlaceholderRepository, workspace::Workspace};

pub(crate) fn render(
    repo: &PlaceholderRepository,
    repo_names: &[String],
    selected_repository: usize,
    collapsed: bool,
    cx: &mut Context<Workspace>,
) -> Sidebar<SidebarMenu> {
    let active_branch = repo
        .head
        .branch()
        .map(|branch| Reference::LocalBranch(branch.clone()));

    let items = vec![
        working_item(repo),
        branches_item(repo, active_branch.as_ref()),
        remotes_item(repo, active_branch.as_ref()),
        tags_item(repo),
        stashes_item(repo),
    ];

    let workspace = cx.entity().downgrade();

    Sidebar::new("repository-sidebar")
        .collapsed(collapsed)
        .header(repository_header(
            cx,
            workspace,
            &repo.name,
            repo_names,
            selected_repository,
            collapsed,
        ))
        .child(SidebarMenu::new().children(items))
}

fn repository_header(
    cx: &App,
    workspace: WeakEntity<Workspace>,
    repo_name: &str,
    repo_names: &[String],
    selected: usize,
    collapsed: bool,
) -> impl IntoElement {
    let names = repo_names.to_vec();

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
                    .child(repo_name.to_string()),
            )
            .child(Icon::new(IconName::ChevronsUpDown).size_4().flex_shrink_0());
    }

    header.dropdown_menu(move |menu, _, _| {
        names.iter().enumerate().fold(menu, |menu, (index, name)| {
            let workspace = workspace.clone();
            menu.item(
                PopupMenuItem::new(name.clone())
                    .checked(index == selected)
                    .on_click(move |_, _, cx| {
                        let _ = workspace
                            .update(cx, |workspace, cx| workspace.select_repository(index, cx));
                    }),
            )
        })
    })
}

fn working_item(repo: &PlaceholderRepository) -> SidebarMenuItem {
    let mut item = SidebarMenuItem::new("Working").icon(IconName::FolderOpen);
    let change_count = repo.working_tree_changes.len();

    if change_count > 0 {
        item = item
            .suffix(move |_, _| div().text_xs().child(change_count.to_string()))
            .children(repo.working_tree_changes.iter().map(file_change_item));
    }

    item
}

fn file_change_item(patch: &FilePatch) -> SidebarMenuItem {
    let path = patch
        .display_path()
        .map(|path| path.display().to_string())
        .unwrap_or_default();

    SidebarMenuItem::new(format!("{} {path}", status_glyph(&patch.status)))
}

fn status_glyph(status: &FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "A",
        FileStatus::Deleted => "D",
        FileStatus::Modified => "M",
        FileStatus::Renamed { .. } => "R",
        FileStatus::Copied { .. } => "C",
        FileStatus::TypeChanged => "T",
    }
}

fn tree_item(node: &branch_tree::RefTreeNode, active: Option<&Reference>) -> SidebarMenuItem {
    let is_active = node
        .reference
        .as_ref()
        .zip(active)
        .is_some_and(|(reference, active)| reference == active);

    let mut item = SidebarMenuItem::new(node.segment.clone()).active(is_active);

    if !node.children.is_empty() {
        item = item
            .default_open(true)
            .children(node.children.iter().map(|child| tree_item(child, active)));
    }

    item
}

fn branches_item(repo: &PlaceholderRepository, active: Option<&Reference>) -> SidebarMenuItem {
    let tree = branch_tree::group_by_path(repo.local_branches().cloned());
    let mut item = SidebarMenuItem::new("Branches").icon(IconName::Network);

    if !tree.is_empty() {
        item = item
            .default_open(true)
            .children(tree.iter().map(|node| tree_item(node, active)));
    }

    item
}

fn remotes_item(repo: &PlaceholderRepository, active: Option<&Reference>) -> SidebarMenuItem {
    let tree = branch_tree::group_by_path(repo.remote_branches().cloned());
    let mut item = SidebarMenuItem::new("Remotes").icon(IconName::Globe);

    if !tree.is_empty() {
        item = item.children(tree.iter().map(|node| tree_item(node, active)));
    }

    item
}

fn tags_item(repo: &PlaceholderRepository) -> SidebarMenuItem {
    let tree = branch_tree::group_by_path(repo.tags().cloned());
    let mut item = SidebarMenuItem::new("Tags");

    if !tree.is_empty() {
        item = item.children(tree.iter().map(|node| tree_item(node, None)));
    }

    item
}

fn stashes_item(repo: &PlaceholderRepository) -> SidebarMenuItem {
    let mut item = SidebarMenuItem::new("Stashes");

    if !repo.stashes.is_empty() {
        item = item.children(repo.stashes.iter().map(|stash| {
            SidebarMenuItem::new(format!("stash@{{{}}}: {}", stash.index, stash.message))
        }));
    }

    item
}
