//! Root view of a gitr window: title bar, sidebar, centre/bottom dock and status bar.
//!
//! ```text
//! ┌─ TitleBar : gitr — <repo> · <branch> ──────────────────────────────┐
//! ├───────────────┬────────────────────────────────────────────────────┤
//! │ [repo ▾]      │ centre dock (workstream E fills this)              │
//! │ ▸ Working     │                                                    │
//! │ ▾ Branches    │                                                    │
//! │ ▸ Remotes     ├──────────────── bottom dock ───────────────────────┤
//! │ ▸ Tags        │ (workstream F fills this)                          │
//! │ ▸ Stashes     │                                                    │
//! ├───────────────┴────────────────────────────────────────────────────┤
//! │ StatusBar : N commits · theme toggle                                │
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! The sidebar is plain window chrome, laid out beside [`DockArea`] rather than inside
//! it as a left dock: it is permanent navigation, not a panel a user drags or closes. The
//! dock only ever owns the centre and bottom placements here — see
//! [`install_default_layout`] for exactly where workstream E and F plug in.

use std::time::Duration;

use gpui::{
    App, AppContext as _, Context, Edges, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Task, WeakEntity, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Root, Sizable as _, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockAreaState, DockEvent, DockItem},
    h_flex,
    status_bar::StatusBar,
};

use crate::{
    dock_seam::{SeamKind, SeamPanel},
    persistence,
    placeholder::{self, PlaceholderRepository},
    sidebar,
};

const DOCK_AREA_ID: &str = "gitr-dock";

/// Bumping this invalidates a layout persisted against a different default — see
/// [`install_default_layout`]. Workstreams E and F should bump it the day they replace a
/// [`SeamPanel`] with a real one, so a user's saved layout does not silently restore a
/// dock item that no longer matches what gets built by default.
pub(crate) const DOCK_AREA_VERSION: usize = 1;

const BOTTOM_DOCK_HEIGHT: f32 = 240.;
const SAVE_DEBOUNCE: Duration = Duration::from_secs(10);

/// Root view of a gitr window.
pub struct Workspace {
    dock_area: Entity<DockArea>,
    repositories: Vec<PlaceholderRepository>,
    selected_repository: usize,
    sidebar_collapsed: bool,
    follow_system_theme: bool,
    last_saved_layout: Option<DockAreaState>,
    _save_layout_task: Option<Task<()>>,
    _appearance_subscription: Subscription,
    _dock_subscription: Subscription,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Theme::sync_system_appearance(Some(window), cx);

        let entity = cx.entity();
        let appearance_subscription = window.observe_window_appearance(move |window, cx| {
            if entity.read(cx).follow_system_theme {
                Theme::sync_system_appearance(Some(window), cx);
            }
        });

        let dock_area =
            cx.new(|cx| DockArea::new(DOCK_AREA_ID, Some(DOCK_AREA_VERSION), window, cx));
        let weak_dock_area = dock_area.downgrade();

        let mut restored = false;
        if let Some(state) = persistence::load()
            && state.version == Some(DOCK_AREA_VERSION)
        {
            match dock_area.update(cx, |area, cx| area.load(state, window, cx)) {
                Ok(()) => restored = true,
                Err(error) => {
                    eprintln!("gitr: failed to restore dock layout, using the default: {error:#}")
                }
            }
        }

        if !restored {
            install_default_layout(&weak_dock_area, window, cx);
        }

        let dock_subscription = cx.subscribe_in(
            &dock_area,
            window,
            |this, dock_area, event: &DockEvent, window, cx| {
                if let DockEvent::LayoutChanged = event {
                    this.schedule_save(dock_area, window, cx);
                }
            },
        );

        cx.on_app_quit(|this, cx| {
            let state = this.dock_area.read(cx).dump(cx);
            cx.background_executor().spawn(async move {
                if let Err(error) = persistence::save(&state) {
                    eprintln!("gitr: failed to save dock layout on quit: {error:#}");
                }
            })
        })
        .detach();

        Self {
            dock_area,
            repositories: placeholder::placeholder_repositories(),
            selected_repository: 0,
            sidebar_collapsed: false,
            follow_system_theme: true,
            last_saved_layout: None,
            _save_layout_task: None,
            _appearance_subscription: appearance_subscription,
            _dock_subscription: dock_subscription,
        }
    }

    pub(crate) fn select_repository(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.repositories.len() {
            self.selected_repository = index;
            cx.notify();
        }
    }

    fn schedule_save(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dock_area = dock_area.clone();
        self._save_layout_task = Some(cx.spawn_in(window, async move |workspace, window| {
            window.background_executor().timer(SAVE_DEBOUNCE).await;

            let _ = workspace.update_in(window, move |workspace, _, cx| {
                let state = dock_area.read(cx).dump(cx);
                if workspace.last_saved_layout.as_ref() == Some(&state) {
                    return;
                }

                let to_write = state.clone();
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = persistence::save(&to_write) {
                            eprintln!("gitr: failed to save dock layout: {error:#}");
                        }
                    })
                    .detach();
                workspace.last_saved_layout = Some(state);
            });
        }));
    }
}

/// Installs the default centre/bottom layout: one [`SeamPanel`] each, standing in for the
/// real history and detail views.
///
/// This is the seam. Workstream E replaces `SeamKind::History` here with its own
/// `history::HistoryPanel` (any `Entity<P>` where `P: Panel` — see
/// `gpui_component::dock::Panel`) built with `cx.new(|cx| history::HistoryPanel::new(..))`
/// and wrapped the same way with `DockItem::tab`, then calls `area.set_center(..)` with
/// it. Workstream F does the same for `SeamKind::Detail` and `area.set_bottom_dock(..)`.
/// Both should bump [`DOCK_AREA_VERSION`] when they do.
fn install_default_layout(dock_area: &WeakEntity<DockArea>, window: &mut Window, cx: &mut App) {
    let Some(dock_area_entity) = dock_area.upgrade() else {
        return;
    };

    let history = cx.new(|cx| SeamPanel::new(SeamKind::History, cx));
    let detail = cx.new(|cx| SeamPanel::new(SeamKind::Detail, cx));

    let center = DockItem::tab(history, dock_area, window, cx);
    let bottom = DockItem::tab(detail, dock_area, window, cx);

    dock_area_entity.update(cx, |area, cx| {
        area.set_version(DOCK_AREA_VERSION, window, cx);
        area.set_center(center, window, cx);
        area.set_bottom_dock(bottom, Some(px(BOTTOM_DOCK_HEIGHT)), true, window, cx);
        area.set_dock_collapsible(
            Edges {
                bottom: true,
                ..Default::default()
            },
            window,
            cx,
        );
    });
}

fn title_bar(
    repo: &PlaceholderRepository,
    sidebar_collapsed: bool,
    cx: &mut Context<Workspace>,
) -> TitleBar {
    let branch = repo
        .head
        .branch()
        .map(|branch| branch.to_string())
        .unwrap_or_else(|| "detached HEAD".to_string());
    let title = format!("gitr — {} · {branch}", repo.name);

    TitleBar::new().child(
        h_flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("toggle-sidebar")
                    .ghost()
                    .xsmall()
                    .icon(if sidebar_collapsed {
                        IconName::PanelLeftOpen
                    } else {
                        IconName::PanelLeftClose
                    })
                    .tooltip("Toggle Sidebar")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sidebar_collapsed = !this.sidebar_collapsed;
                        cx.notify();
                    })),
            )
            .child(div().text_sm().child(title)),
    )
}

fn status_bar(repo: &PlaceholderRepository, cx: &mut Context<Workspace>) -> StatusBar {
    let is_dark = cx.theme().is_dark();

    StatusBar::new()
        .left(format!("{} commits", repo.commit_count))
        .right(
            Button::new("toggle-theme")
                .ghost()
                .xsmall()
                .icon(if is_dark {
                    IconName::Sun
                } else {
                    IconName::Moon
                })
                .tooltip(if is_dark {
                    "Switch to light theme"
                } else {
                    "Switch to dark theme"
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.follow_system_theme = false;
                    let next = if cx.theme().is_dark() {
                        ThemeMode::Light
                    } else {
                        ThemeMode::Dark
                    };
                    Theme::change(next, Some(window), cx);
                })),
        )
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let repo_index = self.selected_repository;
        let sidebar_collapsed = self.sidebar_collapsed;
        let repo_names: Vec<String> = self.repositories.iter().map(|r| r.name.clone()).collect();
        let dock_area = self.dock_area.clone();
        let repo = &self.repositories[repo_index];

        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .id("workspace")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(title_bar(repo, sidebar_collapsed, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(sidebar::render(
                        repo,
                        &repo_names,
                        repo_index,
                        sidebar_collapsed,
                        cx,
                    ))
                    .child(div().flex_1().min_w_0().child(dock_area)),
            )
            .child(status_bar(repo, cx))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}
