//! Renders a `Patch` as a scrollable, monospaced diff: one block per file, one row per
//! diff line, coloured by [`format::classify_line`].

use domain::{DiffLine, FilePatch, Hunk, LineOrigin, Patch};
use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _,
    px,
};
use gpui_component::{ActiveTheme as _, scroll::ScrollableElement as _, separator::Separator};

use super::format::{self, FileBody};

const GUTTER_WIDTH: f32 = 44.;
const MARKER_WIDTH: f32 = 16.;

pub(super) fn render(patch: &Patch, cx: &App, scroll_handle: &ScrollHandle) -> AnyElement {
    if patch.files.is_empty() {
        return div()
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("This commit changes nothing.")
            .into_any_element();
    }

    let mono = cx.theme().mono_font_family.clone();

    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id("detail-diff-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .flex()
                .flex_col()
                .children(
                    patch
                        .files
                        .iter()
                        .map(|file| file_block(file, cx, mono.clone())),
                ),
        )
        .vertical_scrollbar(scroll_handle)
        .into_any_element()
}

fn file_block(file: &FilePatch, cx: &App, mono: SharedString) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(Separator::horizontal().color(cx.theme().border))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_1()
                .font_family(mono.clone())
                .text_color(cx.theme().foreground)
                .child(format::file_header(file))
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(format::diff_stat(file)),
                ),
        )
        .child(Separator::horizontal().color(cx.theme().border))
        .child(file_content(file, cx, mono))
}

fn file_content(file: &FilePatch, cx: &App, mono: SharedString) -> AnyElement {
    match format::file_body(file) {
        FileBody::Binary => placeholder_row(cx, "Binary file not shown.").into_any_element(),
        FileBody::NoTextChange => placeholder_row(cx, "No content changes.").into_any_element(),
        FileBody::Hunks(hunks) => div()
            .flex()
            .flex_col()
            .children(hunks.iter().map(|hunk| hunk_block(hunk, cx, mono.clone())))
            .into_any_element(),
    }
}

fn placeholder_row(cx: &App, message: &'static str) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .text_color(cx.theme().muted_foreground)
        .child(message)
}

fn hunk_block(hunk: &Hunk, cx: &App, mono: SharedString) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .px_3()
                .py_1()
                .font_family(mono.clone())
                .text_color(cx.theme().muted_foreground)
                .bg(cx.theme().muted)
                .child(format::hunk_heading(hunk)),
        )
        .children(
            hunk.lines
                .iter()
                .map(|line| line_row(line, cx, mono.clone())),
        )
}

fn line_row(line: &DiffLine, cx: &App, mono: SharedString) -> impl IntoElement {
    let colors = format::classify_line(line.origin, cx.theme());
    let marker = match line.origin {
        LineOrigin::Addition => "+",
        LineOrigin::Deletion => "\u{2212}",
        LineOrigin::Context => " ",
    };

    div()
        .flex()
        .font_family(mono)
        .when_some(colors.background, |element, background| {
            element.bg(background)
        })
        .child(gutter_number(line.old_number, cx))
        .child(gutter_number(line.new_number, cx))
        .child(
            div()
                .w(px(MARKER_WIDTH))
                .flex_shrink_0()
                .text_color(colors.foreground)
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(colors.foreground)
                .child(line.content.clone()),
        )
}

fn gutter_number(number: Option<u32>, cx: &App) -> impl IntoElement {
    div()
        .w(px(GUTTER_WIDTH))
        .flex_shrink_0()
        .text_color(cx.theme().muted_foreground)
        .child(number.map(|n| n.to_string()).unwrap_or_default())
}
