//! The right dock's commit detail panel: a fixed metadata header, a capped-height
//! description region, and the diff in a real code editor beneath it.
//!
//! [`DetailPanel`] renders exactly the [`LoadState`] it is handed — it never reads a
//! repository itself. [`metadata::render_header`] is the part that never scrolls away —
//! Subject, ID, Parents and Author. [`format`] holds the logic pulled out of both
//! [`metadata`] and [`diff`] so it can be unit-tested without a window, in particular
//! [`format::unified_diff_text`], which reconstructs the text fed to the diff editor.
//!
//! The diff editor is a persistent [`EditorState`] entity owned by [`DetailPanel`]
//! rather than rebuilt every render, because updating it — like creating it — needs a
//! `&mut Window`, which only [`DetailPanel::new`] and [`Render::render`] receive.
//! [`DetailPanel::set_detail`] cannot reach one, so it stages the reconstructed text in
//! `pending_diff` and [`Render::render`] flushes it into the entity on the next frame,
//! before building this render pass's element tree.

mod diff;
mod format;
mod metadata;

use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window, div,
};
use gpui_component::{
    ActiveTheme as _,
    alert::Alert,
    dock::{Panel, PanelEvent},
    input::EditorState,
    spinner::Spinner,
};

use crate::repository::{CommitDetail, LoadState};

pub struct DetailPanel {
    detail: LoadState<Arc<CommitDetail>>,
    pending_diff: Option<SharedString>,
    diff_editor: Entity<EditorState>,
    description_scroll_handle: ScrollHandle,
    focus_handle: FocusHandle,
}

impl DetailPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let diff_editor = cx.new(|cx| EditorState::new(window, cx).language("diff"));
        Self {
            detail: LoadState::Idle,
            pending_diff: None,
            diff_editor,
            description_scroll_handle: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_detail(&mut self, detail: LoadState<Arc<CommitDetail>>, cx: &mut Context<Self>) {
        if let LoadState::Ready(commit_detail) = &detail
            && !commit_detail.patch.files.is_empty()
        {
            self.pending_diff = Some(format::unified_diff_text(&commit_detail.patch).into());
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(text) = self.pending_diff.take() {
            self.diff_editor
                .update(cx, |state, cx| state.set_value(text, window, cx));
        }

        match &self.detail {
            LoadState::Idle => centered_message(cx, "Select a commit to see its details."),
            LoadState::Loading => loading_state(cx),
            LoadState::Failed(message) => failed_state(message),
            LoadState::Ready(detail) => ready_state(
                detail,
                &self.diff_editor,
                &self.description_scroll_handle,
                cx,
            ),
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

fn ready_state(
    detail: &CommitDetail,
    diff_editor: &Entity<EditorState>,
    description_scroll_handle: &ScrollHandle,
    cx: &App,
) -> AnyElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(metadata::render_header(&detail.commit, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .flex()
                .flex_col()
                .children(metadata::render_description(
                    &detail.commit,
                    description_scroll_handle,
                    cx,
                ))
                .child(div().flex_1().min_h_0().min_w_0().child(diff::render(
                    &detail.patch,
                    diff_editor,
                    cx,
                ))),
        )
        .into_any_element()
}
