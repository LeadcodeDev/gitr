//! The bottom dock's commit detail panel: a metadata header plus a scrollable diff viewer.
//!
//! [`DetailPanel`] renders exactly the [`LoadState`] it is handed — it never reads a
//! repository itself. See [`metadata`] for the header and [`diff`] for the patch viewer;
//! [`format`] holds the logic pulled out of both so it can be unit-tested without a window.

mod diff;
mod format;
mod metadata;

use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, ScrollHandle, Styled as _, Window, div,
};
use gpui_component::{
    ActiveTheme as _,
    alert::Alert,
    dock::{Panel, PanelEvent},
    spinner::Spinner,
};

use crate::repository::{CommitDetail, LoadState};

pub struct DetailPanel {
    detail: LoadState<Arc<CommitDetail>>,
    scroll_handle: ScrollHandle,
    focus_handle: FocusHandle,
}

impl DetailPanel {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            detail: LoadState::Idle,
            scroll_handle: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_detail(&mut self, detail: LoadState<Arc<CommitDetail>>, cx: &mut Context<Self>) {
        self.detail = detail;
        cx.notify();
    }
}

impl Panel for DetailPanel {
    fn panel_name(&self) -> &'static str {
        "DetailPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Detail"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl EventEmitter<PanelEvent> for DetailPanel {}

impl Focusable for DetailPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DetailPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.detail {
            LoadState::Idle => centered_message(cx, "Select a commit to see its details."),
            LoadState::Loading => loading_state(cx),
            LoadState::Failed(message) => failed_state(message),
            LoadState::Ready(detail) => ready_state(detail, &self.scroll_handle, cx),
        }
    }
}

fn centered_message(cx: &App, message: &str) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child(message.to_string())
        .into_any_element()
}

fn loading_state(cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .child(Spinner::new())
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child("Loading commit\u{2026}"),
        )
        .into_any_element()
}

fn failed_state(message: &str) -> AnyElement {
    div()
        .size_full()
        .p_3()
        .child(Alert::error("detail-panel-error", message.to_string()))
        .into_any_element()
}

fn ready_state(detail: &CommitDetail, scroll_handle: &ScrollHandle, cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(metadata::render(&detail.commit, cx))
        .child(diff::render(&detail.patch, cx, scroll_handle))
        .into_any_element()
}
