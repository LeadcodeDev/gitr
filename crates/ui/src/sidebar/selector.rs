//! The repository bar's project selector: the control that used to just show the open
//! repository's name now lists every remembered project, lets the user search across all
//! of them, and carries the two ways to add one — from disk or from a public URL. This is
//! also where the project switcher workstream #15 parked in the title bar belongs; see
//! `crates/ui/src/workspace.rs`'s `title_bar`, which no longer renders one.
//!
//! [`SelectorInputs`] is the render-time state [`crate::workspace::Workspace`] owns and
//! this module only borrows: the search and "add from URL" text entities, the list's
//! scroll position, and whether a clone or a synchronise is currently in flight. Every
//! click here reaches back into [`Workspace`] through a [`WeakEntity`] exactly like
//! `crate::sidebar::tree`'s reference rows already do — there is no separate event type,
//! because there is no state here that outlives one render pass.

use std::time::SystemTime;

use gpui::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Selectable, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    popover::Popover,
    scroll::ScrollableElement as _,
    separator::Separator,
    spinner::Spinner,
    v_flex,
};

use crate::density;
use crate::project::{
    MAX_VISIBLE_PROJECTS, Project, ProjectList, ProjectSource, filter_projects,
    format_last_synchronised,
};
use crate::workspace::Workspace;

/// Render-time state [`Workspace`] owns and hands to [`popover`] by reference — a plain
/// bundle rather than five positional parameters, since every field is read, never
/// written, from this module.
pub(crate) struct SelectorInputs<'a> {
    pub search_input: &'a Entity<InputState>,
    pub url_input: &'a Entity<InputState>,
    pub list_scroll: &'a ScrollHandle,
    pub cloning: Option<&'a str>,
    pub synchronising: bool,
}

/// Wraps an arbitrary row in the [`Selectable`] every [`Popover::trigger`] requires.
///
/// `gpui_component`'s own dropdown triggers (`Button`, `SidebarHeader`) all implement
/// [`Selectable`] already, but the repository bar's accent-icon-square row predates this
/// popover and is a plain styled `div` — this is the few lines needed to reuse it as a
/// trigger rather than rebuild it out of `Button`. `selected` is read by
/// [`gpui_base::Popover`] to keep the row visually "pressed" while its own popup is open;
/// nothing here draws on it directly, it only needs to be stored and handed back.
#[derive(IntoElement)]
struct HeaderTrigger {
    content: AnyElement,
    selected: bool,
}

impl HeaderTrigger {
    fn new(content: impl IntoElement) -> Self {
        Self {
            content: content.into_any_element(),
            selected: false,
        }
    }
}

impl Selectable for HeaderTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for HeaderTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.content
    }
}

/// The repository bar's header: the accent icon, the active project's name (ellipsized,
/// never widening the sidebar), a synchronise control when that project is remote, and
/// the popover — search box, capped scrollable list, and the two "add a project" actions
/// — that opens on a click.
pub(crate) fn popover(
    projects: &ProjectList,
    inputs: SelectorInputs<'_>,
    collapsed: bool,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let active = projects.active_project();
    let name = active
        .map(|project| project.name.clone())
        .unwrap_or_else(|| "gitr".to_string());
    let last_synchronised = match active.map(|project| &project.source) {
        Some(ProjectSource::Remote(remote)) => Some(remote.last_synchronised),
        _ => None,
    };
    let is_remote = last_synchronised.is_some();

    let workspace = cx.entity().downgrade();
    let icon = if is_remote {
        IconName::Globe
    } else {
        IconName::FolderOpen
    };

    let mut row = h_flex()
        .id("repository-header-row")
        .w_full()
        .items_center()
        .gap_2()
        .p_2()
        .rounded(cx.theme().radius)
        .hover(|this| {
            this.bg(cx.theme().tokens.sidebar_accent)
                .text_color(cx.theme().sidebar_accent_foreground)
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size_8()
                .flex_shrink_0()
                .rounded(cx.theme().radius)
                .bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
                .child(Icon::new(icon)),
        );

    if !collapsed {
        let mut label_column = v_flex().flex_1().min_w_0().child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_sm()
                .font_medium()
                .child(name),
        );
        if let Some(last_synchronised) = last_synchronised {
            label_column = label_column.child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format_last_synchronised(
                        last_synchronised,
                        SystemTime::now(),
                    )),
            );
        }
        row = row.child(label_column);

        if is_remote {
            row = row.child(sync_button(
                &workspace,
                last_synchronised.flatten(),
                inputs.synchronising,
            ));
        }

        row = row.child(Icon::new(IconName::ChevronsUpDown).size_4().flex_shrink_0());
    }

    let query = inputs.search_input.read(cx).value().to_string();
    let matches: Vec<Project> = filter_projects(&projects.projects, &query)
        .into_iter()
        .cloned()
        .collect();

    let content_args = PopoverContentArgs {
        workspace,
        matches,
        active: projects.active.clone(),
        search_input: inputs.search_input.clone(),
        url_input: inputs.url_input.clone(),
        list_scroll: inputs.list_scroll.clone(),
        cloning: inputs.cloning.map(str::to_string),
    };

    Popover::new("project-selector")
        .trigger(HeaderTrigger::new(row))
        .content(move |_, window, cx| popover_content(&content_args, window, cx))
}

/// Everything [`popover_content`] needs, bundled so the `Popover::content` closure that
/// builds it stays under clippy's argument-count lint.
struct PopoverContentArgs {
    workspace: WeakEntity<Workspace>,
    matches: Vec<Project>,
    active: Option<ProjectSource>,
    search_input: Entity<InputState>,
    url_input: Entity<InputState>,
    list_scroll: ScrollHandle,
    cloning: Option<String>,
}

/// The popover's body: the search box, the capped and scrollable project list, and the
/// two "add a project" actions beneath it.
fn popover_content(args: &PopoverContentArgs, _window: &mut Window, cx: &mut App) -> AnyElement {
    let max_height = density::SIDEBAR_ROW_HEIGHT * MAX_VISIBLE_PROJECTS as f32;

    let list: AnyElement = if args.matches.is_empty() {
        div()
            .px_2()
            .py_1()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("No projects match")
            .into_any_element()
    } else {
        div()
            .id("project-selector-list")
            .max_h(max_height)
            .overflow_y_scroll()
            .track_scroll(&args.list_scroll)
            .flex()
            .flex_col()
            .children(
                args.matches
                    .iter()
                    .map(|project| project_row(&args.workspace, project, args.active.as_ref(), cx)),
            )
            .into_any_element()
    };

    v_flex()
        .id("project-selector")
        .w(px(280.))
        .gap_2()
        .child(
            Input::new(&args.search_input)
                .small()
                .cleanable(true)
                .prefix(Icon::new(IconName::Search).small())
                .w_full(),
        )
        .child(
            div()
                .relative()
                .max_h(max_height)
                .child(list)
                .vertical_scrollbar(&args.list_scroll),
        )
        .child(Separator::horizontal())
        .child(open_from_disk_button(&args.workspace))
        .child(
            Input::new(&args.url_input)
                .small()
                .disabled(args.cloning.is_some())
                .cleanable(true)
                .prefix(Icon::new(IconName::Globe).small())
                .w_full(),
        )
        .children(args.cloning.as_deref().map(cloning_row))
        .into_any_element()
}

fn project_row(
    workspace: &WeakEntity<Workspace>,
    project: &Project,
    active: Option<&ProjectSource>,
    cx: &App,
) -> impl IntoElement {
    let is_active = active == Some(&project.source);
    let is_remote = matches!(project.source, ProjectSource::Remote(_));
    let icon = if is_remote {
        IconName::Globe
    } else {
        IconName::FolderOpen
    };
    let workspace = workspace.clone();
    let source = project.source.clone();
    let row_id = SharedString::from(format!("project-row-{}", project.name));

    h_flex()
        .id(row_id)
        .w_full()
        .h(density::SIDEBAR_ROW_HEIGHT)
        .flex_shrink_0()
        .items_center()
        .gap_2()
        .px_2()
        .rounded(cx.theme().radius)
        .when(is_active, |this| {
            this.bg(cx.theme().tokens.sidebar_accent)
                .text_color(cx.theme().sidebar_accent_foreground)
        })
        .when(!is_active, |this| {
            this.hover(|this| {
                this.bg(cx.theme().sidebar_accent.opacity(0.8))
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
        })
        .child(Icon::new(icon).size_4().flex_shrink_0())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_sm()
                .child(project.name.clone()),
        )
        .when(is_active, |this| {
            this.child(Icon::new(IconName::Check).size_4().flex_shrink_0())
        })
        .on_click(move |_, window, cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            let source = source.clone();
            workspace.update(cx, |workspace, cx| {
                workspace.switch_to(source, window, cx);
            });
        })
}

fn sync_button(
    workspace: &WeakEntity<Workspace>,
    last_synchronised: Option<SystemTime>,
    synchronising: bool,
) -> impl IntoElement {
    let workspace = workspace.clone();
    let tooltip = format_last_synchronised(last_synchronised, SystemTime::now());

    Button::new("synchronise-project")
        .ghost()
        .xsmall()
        .icon(IconName::LoaderCircle)
        .loading(synchronising)
        .tooltip(tooltip)
        .on_click(move |_, window, cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            workspace.update(cx, |workspace, cx| {
                workspace.synchronise_active_project(window, cx);
            });
        })
}

fn open_from_disk_button(workspace: &WeakEntity<Workspace>) -> impl IntoElement {
    let workspace = workspace.clone();

    Button::new("open-from-disk")
        .ghost()
        .small()
        .icon(IconName::FolderOpen)
        .label("Open from Disk…")
        .w_full()
        .on_click(move |_, window, cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            workspace.update(cx, |workspace, cx| {
                workspace.open_from_disk(window, cx);
            });
        })
}

fn cloning_row(url: &str) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .text_xs()
        .child(Spinner::new())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(format!("Cloning {url}…")),
        )
}
