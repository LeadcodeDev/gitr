//! Views built on gpui-component.
//!
//! Ports are synchronous and blocking, so every call into `vcs` from here runs on
//! `cx.background_executor()`. Nothing in this crate blocks the frame thread, and no
//! long operation opens a modal — that pairing is what makes GitX crash on
//! `assert(currentModalSheet == nil)` when two network operations overlap.
//!
//! [`workspace::Workspace`] is the window's root view. It owns a
//! [`repository::RepositoryState`], the single point where this crate reads a
//! `domain::RepositoryReader`, and pushes what it loads into [`history::HistoryPanel`] and
//! [`detail::DetailPanel`] — see `install_default_layout` in `workspace` for exactly
//! where those two panels plug into the dock.

pub mod detail;
pub mod graph_palette;
pub mod history;
pub mod persistence;
pub mod repository;
pub mod sidebar;
pub mod workspace;

pub use workspace::Workspace;

use gpui::{App, AppContext as _};
use gpui_component::dock::register_panel;

use detail::DetailPanel;
use history::HistoryPanel;

/// Registers everything this crate owns in the global `App` state.
///
/// Must run after `gpui_component::init(cx)` and before a saved dock layout is loaded:
/// this teaches `gpui_component`'s `PanelRegistry` how to rebuild [`HistoryPanel`] and
/// [`DetailPanel`] from persisted JSON, and a layout referencing an unregistered panel
/// name silently falls back to `gpui_component`'s `InvalidPanel`.
pub fn init(cx: &mut App) {
    register_panel(cx, "HistoryPanel", |_, _, _, window, cx| {
        Box::new(cx.new(|cx| HistoryPanel::new(window, cx)))
    });
    register_panel(cx, "DetailPanel", |_, _, _, window, cx| {
        Box::new(cx.new(|cx| DetailPanel::new(window, cx)))
    });
}
