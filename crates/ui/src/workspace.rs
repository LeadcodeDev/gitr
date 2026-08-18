//! Root view of a gitr window: title bar, sidebar, centre split and status bar.
//!
//! ```text
//! ┌─ TitleBar : gitr — <repo> · <branch> ───────────────────── [theme ▾] ────┐
//! ├───────────────┬──────────────────────────────┬──────────────────────────┤
//! │ [repo ▾]      │ centre split: HistoryPanel    │ DetailPanel              │
//! │ ▸ Working     │                               │                         │
//! │ ▾ Branches    │                               │                         │
//! │ ▸ Remotes     │                               │                         │
//! │ ▸ Tags        │                               │                         │
//! │ ▸ Stashes     │                               │                         │
//! ├───────────────┴──────────────────────────────┴──────────────────────────┤
//! │ StatusBar : N commits                                                    │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! The sidebar is plain window chrome, laid out beside [`DockArea`] rather than inside
//! it as a left dock: it is permanent navigation, not a panel a user drags or closes. The
//! dock's only placement here is the centre, holding [`HistoryPanel`] and [`DetailPanel`]
//! as siblings of one horizontal [`DockItem::Split`] rather than as a dock and a sidecar —
//! see [`install_default_layout`] for exactly how. The detail panel's half of that split
//! can be entirely absent from the tree; [`Workspace::reveal_detail`] is what puts it back.
//!
//! [`Workspace`] owns the single [`RepositoryState`] the window currently has open, plus
//! the full [`ProjectList`] of every project the user has added. Only the active project
//! ever has a live [`RepositoryState`] — switching, in [`Workspace::open_project`], drops
//! the old one (and with it its background reads and its filesystem watcher thread)
//! before the new one is created, so there is never a moment with two watchers running
//! or with a frame mixing one project's history under another's name. Every
//! `RepositoryEvent` the active repository emits is pushed into the panels here; nothing
//! downstream reads a repository itself.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use domain::{HeadState, HistoryScope, Reference};
use gpui::{
    Animation, AnimationExt as _, App, AppContext as _, Axis, Context, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, PathPromptOptions, Render,
    Styled as _, Subscription, Task, WeakEntity, Window, div, ease_in_out, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Root, Sizable as _, Theme, ThemeMode, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockAreaState, DockEvent, DockItem, Panel, StackPanel, TabPanel},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::NotificationType,
    status_bar::StatusBar,
};

use crate::{
    detail::DetailPanel,
    history::{HistoryPanel, HistoryPanelEvent},
    persistence,
    project::{Project, ProjectList, ProjectSource, display_name, resolve_repository_root},
    repository::{History, HistoryFilter, LoadState, RepositoryEvent, RepositoryState},
    sidebar,
    theme_preference::ThemePreference,
};

const DOCK_AREA_ID: &str = "gitr-dock";

/// Bumping this invalidates a layout persisted against a different default — see
/// [`install_default_layout`]. Bumped from `1` to `2` when the centre and bottom docks
/// started holding [`HistoryPanel`] and [`DetailPanel`] instead of the placeholder seam
/// panels, from `2` to `3` when [`DetailPanel`] moved from the bottom dock to the right
/// dock, and from `3` to `4` when the right dock was replaced by a horizontal split in the
/// centre. A `3` file has no `right_dock` for the new default to read: restoring it as-is
/// would leave [`DetailPanel`] absent from the centre with nothing left to reveal it,
/// silently reproducing a single-panel history view instead of the sibling split. The
/// version gate below never attempts that load at all.
pub(crate) const DOCK_AREA_VERSION: usize = 4;

/// Starting width of the split's detail child, and the width it is restored to on
/// reveal. A commit's metadata header reads as a handful of label/value rows regardless
/// of width, but the diff beneath it is prose-like — 480px is narrow enough to keep the
/// history table's own columns comfortable next to it, wide enough that an 80-column
/// diff line doesn't wrap.
const DETAIL_DOCK_WIDTH: f32 = 480.;

/// What [`install_default_layout`] and [`restore_panels`] hand back to
/// [`Workspace::new`]: both content panels, plus the detail panel's tab group when it
/// is currently attached to the centre split.
type WorkspacePanels = (
    Entity<HistoryPanel>,
    Entity<DetailPanel>,
    Option<Entity<TabPanel>>,
);

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
    projects: ProjectList,
    repository: Entity<RepositoryState>,
    history_panel: Entity<HistoryPanel>,
    detail_panel: Entity<DetailPanel>,
    /// The detail panel's tab group inside the centre split, if currently attached.
    ///
    /// [`StackPanel`] exposes no way to ask whether a given panel is one of its
    /// children, so this is the only record of it. Every place that adds or removes
    /// the tab group updates this in the same call, which is what keeps a second,
    /// independently-drifting notion of "is it visible" from ever existing.
    detail_slot: Option<Entity<TabPanel>>,
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
    _window_closed_subscription: Subscription,
}

impl Workspace {
    /// Builds the window on `projects.active_project()`.
    ///
    /// `.expect(..)` below is provably infallible, not merely assumed: the only caller,
    /// `crates/gitr/src/main.rs`, seeds one project from the CLI argument or the working
    /// directory whenever the persisted list it loads is empty, before this is ever
    /// called, so `projects` always has an active project by the time it gets here.
    ///
    /// gitr opens exactly one window, so there is no "close this one, keep the others
    /// running" case for it to leave open: `App::on_window_closed` firing for any window
    /// closing is indistinguishable from *the* window closing here, and the `cx.quit()`
    /// it calls runs the `on_app_quit` handler registered below before the process ends,
    /// so the dock layout still gets its last save.
    pub fn new(projects: ProjectList, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme_preference = theme_preference_at_startup();
        apply_theme_preference(theme_preference, window, cx);

        let entity = cx.entity();
        let appearance_subscription = window.observe_window_appearance(move |window, cx| {
            if entity.read(cx).theme_preference.follows_system() {
                Theme::sync_system_appearance(Some(window), cx);
            }
        });

        let active_project = projects
            .active_project()
            .cloned()
            .expect("crates/gitr/src/main.rs never opens a window on an empty project list");
        let active_path = match &active_project.source {
            ProjectSource::Local(path) => path.clone(),
        };
        let repository = cx.new(|cx| RepositoryState::open(active_path, cx));

        let dock_area =
            cx.new(|cx| DockArea::new(DOCK_AREA_ID, Some(DOCK_AREA_VERSION), window, cx));

        let mut restored = None;
        if let Some(state) = persistence::load()
            && state.version == Some(DOCK_AREA_VERSION)
        {
            match dock_area.update(cx, |area, cx| area.load(state, window, cx)) {
                Ok(()) => {
                    restored = restore_panels(&dock_area, window, cx);
                    if restored.is_none() {
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

        let (history_panel, detail_panel, detail_slot) =
            restored.unwrap_or_else(|| install_default_layout(&dock_area, window, cx));

        sync_panels_from_repository(&repository, &history_panel, &detail_panel, cx);

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

        let window_closed_subscription = cx.on_window_closed(|cx, _window_id| cx.quit());

        Self {
            dock_area,
            projects,
            repository,
            history_panel,
            detail_panel,
            detail_slot,
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
            _window_closed_subscription: window_closed_subscription,
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            HistoryPanelEvent::Selected(id) => {
                self.repository
                    .update(cx, |repository, cx| repository.select(Some(*id), cx));
            }
            HistoryPanelEvent::DoubleClicked(id) => {
                self.repository
                    .update(cx, |repository, cx| repository.select(Some(*id), cx));
                self.reveal_detail(window, cx);
            }
            HistoryPanelEvent::FilterChanged(filter) => {
                self.repository.update(cx, |repository, cx| {
                    repository.set_filter(filter.clone(), cx)
                });
            }
        }
    }

    /// Reattaches the detail panel's tab group to the centre split if it is currently
    /// absent, and does nothing if it is already attached — a double click on a
    /// commit while the detail panel is already showing must never resize or flicker
    /// it, only select.
    fn reveal_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.detail_slot.is_some() {
            return;
        }

        let Some(content_split) = center_split(&self.dock_area, cx) else {
            eprintln!("gitr: workspace centre is not a split, cannot reveal the detail panel");
            return;
        };

        let weak_dock_area = self.dock_area.downgrade();
        let detail_item = DockItem::tab(self.detail_panel.clone(), &weak_dock_area, window, cx);
        let detail_tab_panel = tab_panel_view(&detail_item);

        content_split.update(cx, |split, cx| {
            split.add_panel(
                detail_item.view(),
                Some(px(DETAIL_DOCK_WIDTH)),
                weak_dock_area,
                window,
                cx,
            );
        });

        self.detail_slot = Some(detail_tab_panel);
    }

    /// Detaches the detail panel from the centre split, giving its width back to the
    /// history.
    ///
    /// Removal rather than collapsing: [`StackPanel`] gates a child's width on
    /// [`Panel::visible`], which [`TabPanel`] derives from its children and never from its
    /// own collapsed flag, so a collapsed panel keeps its column and merely empties it.
    fn hide_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slot) = self.detail_slot.take() else {
            return;
        };
        let Some(content_split) = center_split(&self.dock_area, cx) else {
            self.detail_slot = Some(slot);
            eprintln!("gitr: workspace centre is not a split, cannot hide the detail panel");
            return;
        };

        content_split.update(cx, |split, cx| {
            split.remove_panel(Arc::new(slot), window, cx);
        });
    }

    fn toggle_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.detail_slot.is_some() {
            self.hide_detail(window, cx);
        } else {
            self.reveal_detail(window, cx);
        }
        cx.notify();
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

    /// Replaces the live repository with `project`'s, and nothing else: `self.projects`
    /// must already name `project` as active by the time this runs.
    ///
    /// The old [`RepositoryState`] is dropped the instant `self.repository` is
    /// reassigned — with it go its background reads, its filesystem watcher thread, and
    /// (once `self._repository_subscription` is reassigned too) its event subscription.
    /// [`HistoryPanel`] and [`DetailPanel`] are reset to the new repository's starting
    /// `Loading`/`Idle` state in this same call, and the history's scope and search
    /// filter reset to their defaults — a scope naming a branch from the old repository
    /// would not resolve against the new one. There is no frame in which the old
    /// project's rows, detail, or filter are still showing under the new project's name.
    fn open_project(&mut self, project: &Project, window: &mut Window, cx: &mut Context<Self>) {
        let path = match &project.source {
            ProjectSource::Local(path) => path.clone(),
        };

        let repository = cx.new(|cx| RepositoryState::open(path, cx));
        sync_panels_from_repository(&repository, &self.history_panel, &self.detail_panel, cx);
        self.history_panel
            .update(cx, |panel, cx| panel.reset_for_new_repository(cx));

        self._repository_subscription =
            cx.subscribe_in(&repository, window, Self::on_repository_event);
        self.repository = repository;
    }

    /// Adds `project` to the remembered list if it is not already there, makes it
    /// active, persists the list, and — unless it was already the active project —
    /// opens it. Used by the "open from disk" flow, which may be naming a project this
    /// list has never seen. `crates/gitr/src/main.rs` does the equivalent add-or-activate
    /// directly against the persisted [`ProjectList`] for `gitr <dir>`, since that happens
    /// before the window, and this method, exist.
    fn activate_project(&mut self, project: Project, window: &mut Window, cx: &mut Context<Self>) {
        let already_active = self.projects.active.as_ref() == Some(&project.source);
        self.projects.add_or_activate(project.clone());
        self.persist_projects(cx);

        if !already_active {
            self.open_project(&project, window, cx);
        }
        cx.notify();
    }

    /// Handles a click on one of the switcher's project entries.
    ///
    /// `source` is always one [`project_menu_item`] read off `self.projects`, so
    /// [`ProjectList::activate`] finding nothing for it would mean the menu was built
    /// from stale data — reported rather than silently ignored, even though nothing in
    /// this module can currently produce that.
    fn switch_to(&mut self, source: ProjectSource, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.active.as_ref() == Some(&source) {
            return;
        }
        let Some(project) = self.projects.activate(&source).cloned() else {
            eprintln!("gitr: project switcher chose a project no longer in the list");
            return;
        };

        self.persist_projects(cx);
        self.open_project(&project, window, cx);
        cx.notify();
    }

    /// Resolves `path` to a repository root and adds it to the remembered list,
    /// activating it. A path that does not lead to a Git repository is reported through
    /// a notification — never a dialog, never a panic — and the list is left untouched.
    fn add_project_from_disk(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = match resolve_repository_root(&path) {
            Ok(root) => root,
            Err(error) => {
                window.push_notification((NotificationType::Error, error.to_string()), cx);
                return;
            }
        };
        self.activate_project(Project::local(root), window, cx);
    }

    /// Opens the native directory picker and, once the user chooses one, adds and
    /// activates it through [`Self::add_project_from_disk`].
    ///
    /// A cancelled picker, a platform error, or a dropped channel all collapse to the
    /// same "nothing chosen" outcome: there is nothing to add and nothing to report,
    /// exactly like dismissing any other picker without choosing anything.
    fn open_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });

        cx.spawn_in(window, async move |workspace, window| {
            let path = paths.await.ok()?.ok()??.into_iter().next()?;
            let _ = workspace.update_in(window, move |workspace, window, cx| {
                workspace.add_project_from_disk(path, window, cx);
            });
            Some(())
        })
        .detach();
    }

    /// Writes `self.projects` to disk on the background executor, mirroring
    /// [`Self::set_theme_preference`]'s save: a project being added or switched is a
    /// rare, deliberate action, so it is not debounced the way the dock layout's
    /// frequent [`DockEvent::LayoutChanged`] is.
    fn persist_projects(&self, cx: &mut Context<Self>) {
        let projects = self.projects.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = persistence::save_project_list(&projects) {
                    eprintln!("gitr: failed to save project list: {error:#}");
                }
            })
            .detach();
    }
}

/// Pushes `repository`'s current history and detail into `history_panel` and
/// `detail_panel`. Called right after `RepositoryState::open` — while both are still
/// `Loading`/`Idle`, since neither background read has completed yet — so
/// [`Workspace::new`] and [`Workspace::open_project`] start every window and every
/// switch from that same blank slate rather than whatever the previous repository last
/// rendered.
fn sync_panels_from_repository(
    repository: &Entity<RepositoryState>,
    history_panel: &Entity<HistoryPanel>,
    detail_panel: &Entity<DetailPanel>,
    cx: &mut Context<Workspace>,
) {
    let history = repository.read(cx).history().clone();
    let detail = repository.read(cx).detail().clone();
    history_panel.update(cx, |panel, cx| panel.set_history(history, cx));
    detail_panel.update(cx, |panel, cx| panel.set_detail(detail, cx));
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

/// Builds the default centre layout — [`HistoryPanel`] and [`DetailPanel`] as siblings of
/// one horizontal [`DockItem::Split`], the detail child starting at [`DETAIL_DOCK_WIDTH`]
/// — and hands back both content panels plus the detail panel's tab group, so
/// [`Workspace`] always has a handle to push repository state into and to reattach on a
/// later reveal.
fn install_default_layout(
    dock_area: &Entity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> WorkspacePanels {
    let weak_dock_area = dock_area.downgrade();

    let history_panel = cx.new(|cx| HistoryPanel::new(window, cx));
    let detail_panel = cx.new(|cx| DetailPanel::new(window, cx));

    let history_item = DockItem::tab(history_panel.clone(), &weak_dock_area, window, cx);
    let detail_item = DockItem::tab(detail_panel.clone(), &weak_dock_area, window, cx);
    let detail_tab_panel = tab_panel_view(&detail_item);

    let center = DockItem::split_with_sizes(
        Axis::Horizontal,
        vec![history_item, detail_item],
        vec![None, Some(px(DETAIL_DOCK_WIDTH))],
        &weak_dock_area,
        window,
        cx,
    );

    dock_area.update(cx, |area, cx| {
        area.set_version(DOCK_AREA_VERSION, window, cx);
        area.set_center(center, window, cx);
    });

    (history_panel, detail_panel, Some(detail_tab_panel))
}

/// Locates both panels inside a freshly restored `dock_area`.
///
/// [`HistoryPanel`] must be present in every layout this version writes, so its absence
/// means the file predates this shape or was corrupted — the caller falls back to
/// [`install_default_layout`] in that case. [`DetailPanel`] legitimately may not be: a
/// layout saved while it was hidden persists a one-child split, and that is not a
/// failure. When it is missing, a fresh [`DetailPanel`] is created — unattached, ready
/// to be wired into the split the next time [`Workspace::reveal_detail`] runs.
fn restore_panels(
    dock_area: &Entity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> Option<WorkspacePanels> {
    let history_panel = locate_panel::<HistoryPanel>(dock_area, cx)?;

    let center = dock_area.read(cx).center().clone();
    let (detail_panel, detail_slot) = match find_tab_panel::<DetailPanel>(&center) {
        Some((tab_panel, detail_panel)) => (detail_panel, Some(tab_panel)),
        None => (cx.new(|cx| DetailPanel::new(window, cx)), None),
    };

    Some((history_panel, detail_panel, detail_slot))
}

/// The dock area's centre as a [`StackPanel`], if it currently holds one.
///
/// Every centre this module installs or restores is a [`DockItem::Split`]; `None`
/// only guards against a future change installing something else there instead of
/// panicking on it.
fn center_split(dock_area: &Entity<DockArea>, cx: &App) -> Option<Entity<StackPanel>> {
    match dock_area.read(cx).center() {
        DockItem::Split { view, .. } => Some(view.clone()),
        _ => None,
    }
}

/// The [`TabPanel`] a [`DockItem::tab`] call wraps its panel in.
///
/// Infallible for every value this module builds: [`DockItem::tab`] always returns
/// `Self::Tabs`, never another variant.
fn tab_panel_view(item: &DockItem) -> Entity<TabPanel> {
    match item {
        DockItem::Tabs { view, .. } => view.clone(),
        _ => unreachable!("DockItem::tab always returns DockItem::Tabs"),
    }
}

/// Finds `T` and the [`TabPanel`] tab group wrapping it inside `item`.
///
/// [`find_in_item`] returns only the leaf; reattaching a removed tab group to a
/// [`StackPanel`] needs the wrapper itself, which this recurses down to instead.
fn find_tab_panel<T: Panel>(item: &DockItem) -> Option<(Entity<TabPanel>, Entity<T>)> {
    match item {
        DockItem::Tabs { view, items, .. } => items
            .iter()
            .find_map(|panel| panel.view().downcast::<T>().ok())
            .map(|panel| (view.clone(), panel)),
        DockItem::Split { items, .. } => items.iter().find_map(find_tab_panel),
        DockItem::Panel { .. } | DockItem::Tiles { .. } => None,
    }
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

fn title_bar(
    repository_name: &str,
    head: &LoadState<HeadState>,
    sidebar_collapsed: bool,
    detail_visible: bool,
    theme_preference: ThemePreference,
    projects: &ProjectList,
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
                        .small()
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
        .child(
            h_flex()
                .items_center()
                .gap_1()
                .pr_2()
                .child(
                    Button::new("toggle-detail")
                        .ghost()
                        .small()
                        .icon(if detail_visible {
                            IconName::PanelRightClose
                        } else {
                            IconName::PanelRightOpen
                        })
                        .tooltip(if detail_visible {
                            "Hide commit detail"
                        } else {
                            "Show commit detail"
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_detail(window, cx);
                        })),
                )
                .child(project_switcher_control(projects, cx))
                .child(theme_preference_control(theme_preference, cx)),
        )
}

/// The title bar's project control: a button that opens a menu listing every remembered
/// project — checked on whichever is active, one click to switch — plus an "Open…" entry
/// that hands off to [`Workspace::open_from_disk`]. `crates/ui/src/sidebar/mod.rs`'s
/// header carries a dropdown of its own already, left inert for workstream #18 to build
/// into the real selector; this is the functioning stand-in until then, kept in the title
/// bar rather than in a file this workstream was told to leave alone.
fn project_switcher_control(
    projects: &ProjectList,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let workspace = cx.entity().downgrade();
    let active = projects.active.clone();
    let entries = projects.projects.clone();

    Button::new("project-switcher")
        .ghost()
        .small()
        .icon(IconName::FolderOpen)
        .tooltip("Switch Project")
        .dropdown_menu(move |menu, _, _| {
            let menu = entries.iter().fold(menu, |menu, project| {
                menu.item(project_menu_item(&workspace, project, active.as_ref()))
            });
            menu.separator().item(open_from_disk_menu_item(&workspace))
        })
}

fn project_menu_item(
    workspace: &WeakEntity<Workspace>,
    project: &Project,
    active: Option<&ProjectSource>,
) -> PopupMenuItem {
    let workspace = workspace.clone();
    let source = project.source.clone();
    let checked = active == Some(&project.source);
    PopupMenuItem::new(project.name.clone())
        .icon(IconName::FolderOpen)
        .checked(checked)
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

fn open_from_disk_menu_item(workspace: &WeakEntity<Workspace>) -> PopupMenuItem {
    let workspace = workspace.clone();
    PopupMenuItem::new("Open…")
        .icon(IconName::Plus)
        .on_click(move |_, window, cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            workspace.update(cx, |workspace, cx| {
                workspace.open_from_disk(window, cx);
            });
        })
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
        .small()
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
        .on_click(move |_, window, cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            workspace.update(cx, |workspace, cx| {
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
        let repository_name = display_name(&path);

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
                self.detail_slot.is_some(),
                self.theme_preference,
                &self.projects,
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

#[cfg(test)]
mod tests {
    use super::*;

    const SHOWN_CENTER: &str = r#"{
        "version": 4,
        "center": {
            "panel_name": "StackPanel",
            "children": [
                {
                    "panel_name": "TabPanel",
                    "children": [
                        { "panel_name": "HistoryPanel", "children": [], "info": { "panel": null } }
                    ],
                    "info": { "tabs": { "active_index": 0 } }
                },
                {
                    "panel_name": "TabPanel",
                    "children": [
                        { "panel_name": "DetailPanel", "children": [], "info": { "panel": null } }
                    ],
                    "info": { "tabs": { "active_index": 0 } }
                }
            ],
            "info": { "stack": { "sizes": [820.0, 480.0], "axis": 0 } }
        }
    }"#;

    const HIDDEN_CENTER: &str = r#"{
        "version": 4,
        "center": {
            "panel_name": "StackPanel",
            "children": [
                {
                    "panel_name": "TabPanel",
                    "children": [
                        { "panel_name": "HistoryPanel", "children": [], "info": { "panel": null } }
                    ],
                    "info": { "tabs": { "active_index": 0 } }
                }
            ],
            "info": { "stack": { "sizes": [1300.0], "axis": 0 } }
        }
    }"#;

    #[test]
    fn a_layout_saved_with_the_detail_panel_visible_deserializes_into_a_two_child_split() {
        let state: DockAreaState = serde_json::from_str(SHOWN_CENTER).unwrap();

        assert_eq!(state.version, Some(DOCK_AREA_VERSION));
        assert_eq!(state.center.panel_name, "StackPanel");
        assert_eq!(state.center.children.len(), 2);
        assert_eq!(
            state.center.children[0].children[0].panel_name,
            "HistoryPanel"
        );
        assert_eq!(
            state.center.children[1].children[0].panel_name,
            "DetailPanel"
        );
    }

    #[test]
    fn a_layout_saved_with_the_detail_panel_hidden_deserializes_into_a_one_child_split() {
        let state: DockAreaState = serde_json::from_str(HIDDEN_CENTER).unwrap();

        assert_eq!(state.version, Some(DOCK_AREA_VERSION));
        assert_eq!(state.center.panel_name, "StackPanel");
        assert_eq!(state.center.children.len(), 1);
        assert_eq!(
            state.center.children[0].children[0].panel_name,
            "HistoryPanel"
        );
    }
}
