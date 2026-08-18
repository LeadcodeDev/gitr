//! Views built on gpui-component.
//!
//! Ports are synchronous and blocking, so every call into `vcs` from here runs on
//! `cx.background_executor()`. Nothing in this crate blocks the frame thread, and no
//! long operation opens a modal — that pairing is what makes GitX crash on
//! `assert(currentModalSheet == nil)` when two network operations overlap.
//!
//! [`workspace::Workspace`] is the window's root view. `vcs` is not ready yet, so it
//! renders against [`placeholder`]'s hand-made data; swapping that for a real
//! `domain::RepositoryReader` is the integration point once it is. Workstreams E and F
//! plug a history view and a detail view into the centre and bottom docks — see
//! [`dock_seam`] and `install_default_layout` in `workspace` for exactly where.

pub mod dock_seam;
pub mod graph_palette;
pub mod persistence;
pub mod placeholder;
pub mod sidebar;
pub mod workspace;

pub use workspace::Workspace;

use gpui::App;

/// Registers everything this crate owns in the global `App` state.
///
/// Must run after `gpui_component::init(cx)` and before a saved dock layout is loaded:
/// [`dock_seam::register`] teaches `gpui_component`'s `PanelRegistry` how to rebuild this
/// crate's placeholder panels from persisted JSON, and a layout referencing an
/// unregistered panel name silently falls back to `gpui_component`'s `InvalidPanel`.
pub fn init(cx: &mut App) {
    dock_seam::register(cx);
}
