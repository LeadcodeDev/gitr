//! Placeholder panels holding the centre and bottom docks open until workstream E adds
//! `crates/ui/src/history/` and workstream F adds `crates/ui/src/detail/`.
//!
//! Swapping a seam out is a small, local change in [`crate::workspace`]: build the real
//! panel's `Entity` in place of [`SeamPanel::new`] where the default layout is installed,
//! wrap it the same way with `DockItem::tab`, and bump `workspace::DOCK_AREA_VERSION` so
//! a layout persisted against the placeholder is not silently restored over the real
//! panel — [`register`] is what lets that restore attempt happen safely at all, by
//! teaching `gpui_component`'s `PanelRegistry` how to rebuild [`SeamPanel`] by name.

use gpui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div,
};
use gpui_component::{
    ActiveTheme as _,
    dock::{Panel, PanelEvent, register_panel},
};

/// Which dock a [`SeamPanel`] stands in for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamKind {
    History,
    Detail,
}

impl SeamKind {
    /// The name persisted in a saved dock layout. Once shipped, this must not change —
    /// see [`Panel::panel_name`].
    const fn panel_name(self) -> &'static str {
        match self {
            Self::History => "HistorySeam",
            Self::Detail => "DetailSeam",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::History => "History",
            Self::Detail => "Detail",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::History => "Workstream E fills the centre dock with the commit history here.",
            Self::Detail => "Workstream F fills the bottom dock with the commit detail here.",
        }
    }
}

/// A dock panel that renders nothing but a name and a one-line message, standing in for
/// a real view. Not closable: this workstream has no "add panel" affordance to bring one
/// back, so closing the last seam would leave a dock permanently empty.
pub struct SeamPanel {
    kind: SeamKind,
    focus_handle: FocusHandle,
}

impl SeamPanel {
    pub fn new(kind: SeamKind, cx: &mut Context<Self>) -> Self {
        Self {
            kind,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for SeamPanel {
    fn panel_name(&self) -> &'static str {
        self.kind.panel_name()
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.kind.title()
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl EventEmitter<PanelEvent> for SeamPanel {}

impl Focusable for SeamPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SeamPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(self.kind.message())
    }
}

/// Registers both seam panels so a persisted `DockAreaState` can rebuild them on the next
/// launch. Call once, from [`crate::init`], before any saved layout is loaded — a layout
/// referencing an unregistered panel name silently becomes `gpui_component`'s
/// `InvalidPanel` instead.
pub fn register(cx: &mut App) {
    for kind in [SeamKind::History, SeamKind::Detail] {
        register_panel(cx, kind.panel_name(), move |_, _, _, _, cx| {
            Box::new(cx.new(|cx| SeamPanel::new(kind, cx)))
        });
    }
}
