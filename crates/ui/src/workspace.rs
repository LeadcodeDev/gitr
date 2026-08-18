//! Root view of a gitr window: title bar, sidebar, centre/right dock and status bar.
//!
//! ```text
//! ┌─ TitleBar : gitr — <repo> · <branch> ───────────────────── [theme ▾] ────┐
//! ├───────────────┬───────────────────────────────────────┬────────────────┤
//! │ [repo ▾]      │ centre dock: HistoryPanel              │ right dock:    │
//! │ ▸ Working     │                                        │ DetailPanel    │
//! │ ▾ Branches    │                                        │                │
//! │ ▸ Remotes     │                                        │                │
//! │ ▸ Tags        │                                        │                │
//! │ ▸ Stashes     │                                        │                │
//! ├───────────────┴───────────────────────────────────────┴────────────────┤
//! │ StatusBar : N commits                                                    │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! The sidebar is plain window chrome, laid out beside [`DockArea`] rather than inside
//! it as a left dock: it is permanent navigation, not a panel a user drags or closes. The
//! dock only ever owns the centre and right placements here — see
//! [`install_default_layout`] for exactly where the history and detail panels plug in.
//!
//! [`Workspace`] owns the single [`RepositoryState`] this window is open on. Every
//! `RepositoryEvent` it emits is pushed into the panels here; nothing downstream reads a
//! repository itself.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use domain::{HeadState, HistoryScope, Reference};
use gpui::{
    Animation, AnimationExt as _, App, AppContext as _, Context, Edges, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    Task, WeakEntity, Window, div, ease_in_out, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Root, Sizable as _, Theme, ThemeMode, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockAreaState, DockEvent, DockItem, Panel},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::NotificationType,
    status_bar::StatusBar,
};

use crate::{
    detail::DetailPanel,
    history::{HistoryPanel, HistoryPanelEvent},
    persistence,
    repository::{History, HistoryFilter, LoadState, RepositoryEvent, RepositoryState},
    sidebar,
    theme_preference::ThemePreference,
};

const DOCK_AREA_ID: &str = "gitr-dock";

/// Bumping this invalidates a layout persisted against a different default — see
/// [`install_default_layout`]. Bumped from `1` to `2` when the centre and bottom docks
/// started holding [`HistoryPanel`] and [`DetailPanel`] instead of the placeholder seam
/// panels, and from `2` to `3` when [`DetailPanel`] moved from the bottom dock to the
/// right dock: a layout persisted against either older default must not silently restore
/// the detail panel to the bottom.
pub(crate) const DOCK_AREA_VERSION: usize = 3;

/// Width of the right dock holding [`DetailPanel`]. A commit's metadata header reads
/// as a handful of label/value rows regardless of width, but the diff beneath it is
/// prose-like — 480px is narrow enough to keep the history table's own columns
/// comfortable next to it, wide enough that an 80-column diff line doesn't wrap.
const DETAIL_DOCK_WIDTH: f32 = 480.;

const SAVE_DEBOUNCE: Duration = Duration::from_secs(10);

/// How long the cross-fade in [`theme_transition_overlay`] takes to reveal the new theme.
/// Within the 150–250ms window a mode switch reads as instantaneous but not jarring.
const THEME_TRANSITION_DURATION: Duration = Duration::from_millis(200);

/// A theme cross-fade in progress: the overlay paints in the theme being left, on top of
/// the new theme already painted underneath, and fades out.
///
/// `id` must change on every transition. [`gpui::AnimationExt::with_animation`] keys an
/// animation's start time to its element id, so reusing one across transitions would
/// either replay a stale, already-finished animation on the second switch or not restart
/// it at all.
struct ThemeTransition {
    id: usize,
    from_background: Hsla,
}

/// Root view of a gitr window.
pub struct Workspace {
    dock_area: Entity<DockArea>,
    repository: Entity<RepositoryState>,
    history_panel: Entity<HistoryPanel>,
    detail_panel: Entity<DetailPanel>,
    sidebar_collapsed: bool,
    theme_preference: ThemePreference,
    theme_transition: Option<ThemeTransition>,
    next_theme_transition_id: usize,
    last_saved_layout: Option<DockAreaState>,
    _save_layout_task: Option<Task<()>>,
    _appearance_subscription: Subscription,
    _dock_subscription: Subscription,
    _repository_subscription: Subscription,
    _history_panel_subscription: Subscription,
}

impl Workspace {
    pub fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme_preference = theme_preference_at_startup();
        apply_theme_preference(theme_preference, window, cx);

        let entity = cx.entity();
        let appearance_subscription = window.observe_window_appearance(move |window, cx| {
            if entity.read(cx).theme_preference.follows_system() {
                Theme::sync_system_appearance(Some(window), cx);
            }
        });

        let repository = cx.new(|cx| RepositoryState::open(path, cx));

        let dock_area =
            cx.new(|cx| DockArea::new(DOCK_AREA_ID, Some(DOCK_AREA_VERSION), window, cx));

        let mut restored_panels = None;
        if let Some(state) = persistence::load()
            && state.version == Some(DOCK_AREA_VERSION)
        {
            match dock_area.update(cx, |area, cx| area.load(state, window, cx)) {
                Ok(()) => {
                    restored_panels = locate_panel::<HistoryPanel>(&dock_area, cx)
                        .zip(locate_panel::<DetailPanel>(&dock_area, cx));
                    if restored_panels.is_none() {
                        eprintln!(
                            "gitr: restored dock layout is missing a panel this version installs, using the default"
                        );
                    }
                }
                Err(error) => {
                    eprintln!("gitr: failed to restore dock layout, using the default: {error:#}")
                }
            }
        }

        let (history_panel, detail_panel) =
            restored_panels.unwrap_or_else(|| install_default_layout(&dock_area, window, cx));

        let initial_history = repository.read(cx).history().clone();
        let initial_detail = repository.read(cx).detail().clone();
        history_panel.update(cx, |panel, cx| panel.set_history(initial_history, cx));
        detail_panel.update(cx, |panel, cx| panel.set_detail(initial_detail, cx));

        let repository_subscription =
            cx.subscribe_in(&repository, window, Self::on_repository_event);
        let history_panel_subscription =
            cx.subscribe_in(&history_panel, window, Self::on_history_panel_event);

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
            repository,
            history_panel,
            detail_panel,
            sidebar_collapsed: false,
            theme_preference,
            theme_transition: None,
            next_theme_transition_id: 0,
            last_saved_layout: None,
            _save_layout_task: None,
            _appearance_subscription: appearance_subscription,
            _dock_subscription: dock_subscription,
            _repository_subscription: repository_subscription,
            _history_panel_subscription: history_panel_subscription,
        }
    }

    /// Applies `preference`, persists it, and — when it changes which mode is actually
    /// on screen — starts a cross-fade from the mode being left. The comparison against
    /// `cx.theme().mode` happens before either is touched, so picking `System` while the
    /// OS is already in the mode already showing is a no-op that never flashes the
    /// overlay, and so does re-picking the preference already in effect.
    fn set_theme_preference(
        &mut self,
        preference: ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if preference == self.theme_preference {
            return;
        }

        let target_mode = preference
            .explicit_mode()
            .unwrap_or_else(|| ThemeMode::from(window.appearance()));

        if target_mode != cx.theme().mode {
            self.next_theme_transition_id += 1;
            let transition = ThemeTransition {
                id: self.next_theme_transition_id,
                from_background: cx.theme().background,
            };
            self.theme_transition = Some(transition);

            let transition_id = self.next_theme_transition_id;
            cx.spawn_in(window, async move |workspace, window| {
                window
                    .background_executor()
                    .timer(THEME_TRANSITION_DURATION)
                    .await;
                let _ = workspace.update_in(window, move |workspace, _, cx| {
                    let is_current = workspace
                        .theme_transition
                        .as_ref()
                        .is_some_and(|transition| transition.id == transition_id);
                    if is_current {
                        workspace.theme_transition = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }

        self.theme_preference = preference;
        apply_theme_preference(preference, window, cx);

        cx.background_executor()
            .spawn(async move {
                if let Err(error) = persistence::save_theme_preference(&preference) {
                    eprintln!("gitr: failed to save theme preference: {error:#}");
                }
            })
            .detach();

        cx.notify();
    }

    /// Scopes the history to `reference` and pushes it both to [`RepositoryState`], which
    /// reloads, and directly to [`HistoryPanel`], so its third scope tab appears at once
    /// instead of waiting on the reload's `RepositoryEvent::HistoryChanged` round trip.
    pub(crate) fn filter_by_reference(&mut self, reference: Reference, cx: &mut Context<Self>) {
        let query = self.repository.read(cx).filter().query.clone();
        let filter = HistoryFilter {
            scope: HistoryScope::Single(reference),
            query,
        };
        self.history_panel
            .update(cx, |panel, cx| panel.set_filter(filter.clone(), cx));
        self.repository
            .update(cx, |repository, cx| repository.set_filter(filter, cx));
    }

    fn on_repository_event(
        &mut self,
        repository: &Entity<RepositoryState>,
        event: &RepositoryEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RepositoryEvent::HistoryChanged => {
                let history = repository.read(cx).history().clone();
                self.history_panel
                    .update(cx, |panel, cx| panel.set_history(history, cx));
                cx.notify();
            }
            RepositoryEvent::SelectionChanged => {
                let detail = repository.read(cx).detail().clone();
                self.detail_panel
                    .update(cx, |panel, cx| panel.set_detail(detail, cx));
            }
            RepositoryEvent::Failed(message) => {
                window.push_notification((NotificationType::Error, message.to_string()), cx);
            }
        }
    }

    fn on_history_panel_event(
        &mut self,
        _panel: &Entity<HistoryPanel>,
        event: &HistoryPanelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            HistoryPanelEvent::Selected(id) => {
                self.repository
                    .update(cx, |repository, cx| repository.select(Some(*id), cx));
            }
            HistoryPanelEvent::FilterChanged(filter) => {
                self.repository.update(cx, |repository, cx| {
                    repository.set_filter(filter.clone(), cx)
                });
            }
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

/// Reads the persisted theme preference, logging and defaulting on a genuine failure to
/// parse an existing file. A file that is simply absent — every first launch — is not
/// logged: that is the expected, unremarkable case, exactly as for the dock layout in
/// [`persistence::load`].
fn theme_preference_at_startup() -> ThemePreference {
    let Some(path) = persistence::theme_preference_path() else {
        return ThemePreference::default();
    };
    if !path.exists() {
        return ThemePreference::default();
    }
    persistence::load_theme_preference_from(&path).unwrap_or_else(|error| {
        eprintln!("gitr: failed to read saved theme preference, using the default: {error:#}");
        ThemePreference::default()
    })
}

/// Applies `preference` to the window: an explicit choice pins [`Theme::change`] to it,
/// `System` hands off to [`Theme::sync_system_appearance`] so it reads the OS appearance
/// itself. Shared between the initial launch and every later [`Workspace::set_theme_preference`]
/// call so the two can never disagree on how a preference becomes an applied mode.
fn apply_theme_preference(preference: ThemePreference, window: &mut Window, cx: &mut App) {
    match preference.explicit_mode() {
        Some(mode) => Theme::change(mode, Some(window), cx),
        None => Theme::sync_system_appearance(Some(window), cx),
    }
}

/// Builds the default centre/right layout — a [`HistoryPanel`] tab in the centre and a
/// [`DetailPanel`] tab in the right dock — and hands both back so [`Workspace`] always
/// has a handle to push repository state into, whether this ran because no layout was
/// saved yet or because a saved one turned out not to contain them.
fn install_default_layout(
    dock_area: &Entity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<HistoryPanel>, Entity<DetailPanel>) {
    let weak_dock_area = dock_area.downgrade();

    let history_panel = cx.new(|cx| HistoryPanel::new(window, cx));
    let detail_panel = cx.new(|cx| DetailPanel::new(window, cx));

    let center = DockItem::tab(history_panel.clone(), &weak_dock_area, window, cx);
    let right = DockItem::tab(detail_panel.clone(), &weak_dock_area, window, cx);

    dock_area.update(cx, |area, cx| {
        area.set_version(DOCK_AREA_VERSION, window, cx);
        area.set_center(center, window, cx);
        area.set_right_dock(right, Some(px(DETAIL_DOCK_WIDTH)), true, window, cx);
        area.set_dock_collapsible(
            Edges {
                right: true,
                ..Default::default()
            },
            window,
            cx,
        );
    });

    (history_panel, detail_panel)
}

/// Finds the first `T` panel anywhere in `dock_area`'s docks, regardless of which one a
/// user's drag-and-drop left it in.
fn locate_panel<T: Panel>(dock_area: &Entity<DockArea>, cx: &App) -> Option<Entity<T>> {
    let area = dock_area.read(cx);
    [
        Some(area.center()),
        area.left_dock().map(|dock| dock.read(cx).panel()),
        area.right_dock().map(|dock| dock.read(cx).panel()),
        area.bottom_dock().map(|dock| dock.read(cx).panel()),
    ]
    .into_iter()
    .flatten()
    .find_map(find_in_item)
}

fn find_in_item<T: Panel>(item: &DockItem) -> Option<Entity<T>> {
    match item {
        DockItem::Panel { view, .. } => view.view().downcast::<T>().ok(),
        DockItem::Tabs { items, .. } => items
            .iter()
            .find_map(|view| view.view().downcast::<T>().ok()),
        DockItem::Split { items, .. } => items.iter().find_map(find_in_item),
        DockItem::Tiles { .. } => None,
    }
}

fn repository_display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn title_bar(
    repository_name: &str,
    head: &LoadState<HeadState>,
    sidebar_collapsed: bool,
    theme_preference: ThemePreference,
    cx: &mut Context<Workspace>,
) -> TitleBar {
    let branch = match head {
        LoadState::Ready(head) => head
            .branch()
            .map(|branch| branch.to_string())
            .unwrap_or_else(|| "detached HEAD".to_string()),
        LoadState::Idle | LoadState::Loading => "…".to_string(),
        LoadState::Failed(_) => "unknown".to_string(),
    };
    let title = format!("gitr — {repository_name} · {branch}");

    TitleBar::new()
        .child(
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
        .child(theme_preference_control(theme_preference, cx))
}

/// The title bar's theme control: a button whose icon is the active preference's own
/// (readable at a glance without opening anything) that opens a three-entry menu with a
/// check on the active one.
///
/// A menu was chosen over a button that cycles through the three states: with three
/// states, a cycle can need two clicks to reach the one not adjacent to the current
/// state, and gives no visibility into what the other choices even are. A menu reaches
/// any of the three in one click and shows all three labelled, which matters more for a
/// setting that is touched rarely and half-remembered between visits than a tight click
/// count would.
fn theme_preference_control(
    preference: ThemePreference,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let workspace = cx.entity().downgrade();

    Button::new("theme-preference")
        .ghost()
        .xsmall()
        .icon(preference.icon())
        .tooltip(format!("Theme: {}", preference.label()))
        .dropdown_menu(move |menu, _, _| {
            ThemePreference::ALL.iter().fold(menu, |menu, &option| {
                menu.item(theme_preference_menu_item(&workspace, option, preference))
            })
        })
}

fn theme_preference_menu_item(
    workspace: &WeakEntity<Workspace>,
    option: ThemePreference,
    current: ThemePreference,
) -> PopupMenuItem {
    let workspace = workspace.clone();
    PopupMenuItem::new(option.label())
        .icon(option.icon())
        .checked(option == current)
        .on_click(move |_, _window, cx| {
            let _ = workspace.update_in(cx, move |workspace, window, cx| {
                workspace.set_theme_preference(option, window, cx);
            });
        })
}

fn status_bar(history: &LoadState<Arc<History>>) -> StatusBar {
    let commit_count = match history {
        LoadState::Ready(history) => format!("{} commits", history.len()),
        LoadState::Idle | LoadState::Loading => "Loading…".to_string(),
        LoadState::Failed(_) => "History unavailable".to_string(),
    };

    StatusBar::new().left(commit_count)
}

/// Paints over the window in the theme being left and fades that cover away, so the new
/// theme — already painted underneath — reads as a cross-fade. This fades a single flat
/// background, not every themed element independently; a full interpolation would need
/// to animate each token on every element rather than one overlay, which is a
/// deliberately simpler effect than a true theme morph.
fn theme_transition_overlay(transition: &ThemeTransition) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .bg(transition.from_background)
        .with_animation(
            ("theme-transition", transition.id),
            Animation::new(THEME_TRANSITION_DURATION).with_easing(ease_in_out),
            |overlay, delta| overlay.opacity(1.0 - delta),
        )
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_collapsed = self.sidebar_collapsed;
        let dock_area = self.dock_area.clone();

        let path = self.repository.read(cx).path().to_path_buf();
        let head = self.repository.read(cx).head().clone();
        let references = self.repository.read(cx).references().clone();
        let history = self.repository.read(cx).history().clone();
        let repository_name = repository_display_name(&path);

        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .id("workspace")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(title_bar(
                &repository_name,
                &head,
                sidebar_collapsed,
                self.theme_preference,
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(sidebar::render(
                        &references,
                        &head,
                        &repository_name,
                        sidebar_collapsed,
                        cx,
                    ))
                    .child(div().flex_1().min_w_0().child(dock_area)),
            )
            .child(status_bar(&history))
            .children(self.theme_transition.as_ref().map(theme_transition_overlay))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}
