//! The right dock's commit detail panel: a tab bar switching between commit metadata and
//! the diff, above whichever of the two is selected.
//!
//! [`DetailPanel`] renders exactly the [`LoadState`] it is handed — it never reads a
//! repository itself. [`metadata::render_header`] and [`metadata::render_description`]
//! are the general-information tab's content — Subject, ID, Parents, Author and the
//! commit body. [`format`] holds the logic pulled out of both [`metadata`] and [`diff`]
//! so it can be unit-tested without a window, in particular
//! [`format::unified_diff_text_with_line_ranges`], which reconstructs the text fed to the
//! diff editor together with the byte ranges [`decorations`] turns into line backgrounds.
//!
//! The diff editor is a persistent [`EditorState`] entity owned by [`DetailPanel`] rather
//! than rebuilt every render, because updating it — like creating it — needs a
//! `&mut Window`, which only [`DetailPanel::new`] and [`Render::render`] receive.
//! [`DetailPanel::set_detail`] cannot reach one, so it stages the reconstructed text and
//! ranges in `pending_diff` and [`Render::render`] flushes them into the entity and its
//! decorations collection on the next frame, before building this render pass's element
//! tree. [`DetailPanel::selected_tab`] is never touched by `set_detail`, which is what
//! lets picking a different commit leave the open tab alone.

mod decorations;
mod diff;
mod format;
mod metadata;

use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _,
    alert::Alert,
    dock::{Panel, PanelEvent},
    input::{EditorState, TextDecorationCollection},
    scroll::ScrollableElement as _,
    spinner::Spinner,
    tab::{Tab, TabBar},
};

use crate::repository::{CommitDetail, LoadState};
use format::DiffLineRanges;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DetailTab {
    #[default]
    General,
    Diff,
}

impl DetailTab {
    const ALL: [DetailTab; 2] = [DetailTab::General, DetailTab::Diff];

    fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    fn label(self) -> &'static str {
        match self {
            DetailTab::General => "General",
            DetailTab::Diff => "Diffs",
        }
    }
}

pub struct DetailPanel {
    detail: LoadState<Arc<CommitDetail>>,
    pending_diff: Option<(SharedString, DiffLineRanges)>,
    diff_editor: Entity<EditorState>,
    diff_decorations: TextDecorationCollection,
    selected_tab: DetailTab,
    general_scroll_handle: ScrollHandle,
    focus_handle: FocusHandle,
}

impl DetailPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let diff_editor = cx.new(|cx| EditorState::new(window, cx).language("diff"));
        let diff_decorations = diff_editor.update(cx, |state, cx| {
            state.create_decorations_collection(Vec::new(), cx)
        });
        Self {
            detail: LoadState::Idle,
            pending_diff: None,
            diff_editor,
            diff_decorations,
            selected_tab: DetailTab::default(),
            general_scroll_handle: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_detail(&mut self, detail: LoadState<Arc<CommitDetail>>, cx: &mut Context<Self>) {
        if let LoadState::Ready(commit_detail) = &detail
            && !commit_detail.patch.files.is_empty()
        {
            let (text, ranges) = format::unified_diff_text_with_line_ranges(&commit_detail.patch);
            self.pending_diff = Some((text.into(), ranges));
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
        if let Some((text, ranges)) = self.pending_diff.take() {
            let colors = decorations::line_backgrounds(cx.theme().mode);
            let new_decorations = decorations::build_decorations(&ranges, &colors);
            self.diff_editor
                .update(cx, |state, cx| state.set_value(text, window, cx));
            self.diff_decorations.set(new_decorations, cx);
        }

        let selected_tab = self.selected_tab;
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(tab_bar(selected_tab, cx))
            .child(match &self.detail {
                LoadState::Idle => centered_message(cx, "Select a commit to see its details."),
                LoadState::Loading => loading_state(cx),
                LoadState::Failed(message) => failed_state(message),
                LoadState::Ready(detail) => ready_state(
                    detail,
                    selected_tab,
                    &self.diff_editor,
                    &self.general_scroll_handle,
                    cx,
                ),
            })
    }
}

fn tab_bar(selected: DetailTab, cx: &mut Context<DetailPanel>) -> AnyElement {
    let mut tabs = TabBar::new("detail-tabs")
        .segmented()
        .small()
        .selected_index(selected.index())
        .on_click(cx.listener(|this, index: &usize, _, cx| {
            this.selected_tab = DetailTab::from_index(*index);
            cx.notify();
        }));
    for tab in DetailTab::ALL {
        tabs = tabs.child(Tab::new().label(tab.label()));
    }

    div()
        .flex_shrink_0()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(tabs)
        .into_any_element()
}

fn centered_message(cx: &App, message: &str) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child(message.to_string())
        .into_any_element()
}

fn loading_state(cx: &App) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
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
        .flex_1()
        .min_h_0()
        .p_3()
        .child(Alert::error("detail-panel-error", message.to_string()))
        .into_any_element()
}

fn ready_state(
    detail: &CommitDetail,
    selected_tab: DetailTab,
    diff_editor: &Entity<EditorState>,
    scroll_handle: &ScrollHandle,
    cx: &App,
) -> AnyElement {
    match selected_tab {
        DetailTab::General => general_tab(detail, scroll_handle, cx),
        DetailTab::Diff => diff_tab(detail, diff_editor, cx),
    }
}

/// The whole tab scrolls as one region, header included, rather than pinning the header
/// and giving the body a short scroller of its own. A bounded inner scroller inside an
/// already-tall panel wastes the height it was given and cuts a long message mid-sentence
/// while empty space sits below it.
fn general_tab(detail: &CommitDetail, scroll_handle: &ScrollHandle, cx: &App) -> AnyElement {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(
            div()
                .id("detail-general-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .flex()
                .flex_col()
                .child(metadata::render_header(&detail.commit, cx))
                .children(metadata::render_description(&detail.commit, cx)),
        )
        .vertical_scrollbar(scroll_handle)
        .into_any_element()
}

fn diff_tab(detail: &CommitDetail, diff_editor: &Entity<EditorState>, cx: &App) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(diff::render(&detail.patch, diff_editor, cx))
        .into_any_element()
}
