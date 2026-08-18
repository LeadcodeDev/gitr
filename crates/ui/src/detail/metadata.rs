//! Renders the commit metadata header (subject, identifier, parents and author) and,
//! separately, the commit message body — split because [`render_header`] stays pinned
//! above the detail panel's scroll region while [`render_description`] scrolls with the
//! diff. See `detail::ready_state` for where the two are recombined.

use domain::{Commit, Parents};
use gpui::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::ActiveTheme as _;

use super::format::{abbreviate, format_timestamp};

const LABEL_WIDTH: f32 = 84.;

pub(super) fn render_header(commit: &Commit, cx: &App) -> impl IntoElement {
    let mono = cx.theme().mono_font_family.clone();

    let mut rows = vec![
        row("Subject", div().child(commit.summary.clone()), cx),
        row(
            "ID",
            div().font_family(mono.clone()).child(commit.id.to_string()),
            cx,
        ),
    ];

    if let Some(parents) = parents_line(&commit.parents) {
        rows.push(row("Parents", div().font_family(mono).child(parents), cx));
    }

    rows.push(row("Author", div().child(author_line(commit)), cx));

    div().flex().flex_col().gap_1().p_3().children(rows)
}

/// The commit message body, if it has one beyond the subject line — `None` renders
/// nothing rather than an empty scroll-region row.
pub(super) fn render_description(commit: &Commit, cx: &App) -> Option<AnyElement> {
    let body = commit.body.trim();
    if body.is_empty() {
        return None;
    }

    Some(
        div()
            .px_3()
            .py_3()
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(body.to_string())
            .into_any_element(),
    )
}

fn row(label: &'static str, value: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap_2()
        .text_sm()
        .child(
            div()
                .w(px(LABEL_WIDTH))
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(cx.theme().foreground)
                .child(value),
        )
        .into_any_element()
}

fn author_line(commit: &Commit) -> String {
    let signature = &commit.author;
    format!(
        "{} <{}>    {}",
        signature.name,
        signature.email,
        format_timestamp(signature.time)
    )
}

fn parents_line(parents: &Parents) -> Option<String> {
    if parents.is_empty() {
        return None;
    }
    Some(
        parents
            .iter()
            .map(abbreviate)
            .collect::<Vec<_>>()
            .join(", "),
    )
}
