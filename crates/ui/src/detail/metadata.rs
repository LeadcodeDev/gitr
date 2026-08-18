//! Renders the commit metadata header (subject, identifier, parents and author) and,
//! separately, the commit message body — split because the header renders first
//! above the detail panel's scroll region while [`render_description`] scrolls
//! together with the header inside the tab's own scroll region, so an unusually
//! long body cannot squeeze the diff editor beneath it out of the panel. See
//! `detail::ready_state` for where the two are recombined.
//!
//! Every value here goes through [`gpui_component::text::markdown`] rather than a plain
//! `div`, which is what makes the commit's identifier — the thing most worth copying in
//! this panel — selectable.

use domain::{Commit, Parents};
use gpui::{AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, div, px};
use gpui_component::{ActiveTheme as _, text};

use super::format::{abbreviate, escape_markdown, format_timestamp};

const LABEL_WIDTH: f32 = 84.;

pub(super) fn render_header(commit: &Commit, cx: &App) -> impl IntoElement {
    let mono = cx.theme().mono_font_family.clone();

    let mut rows = vec![
        row("Subject", selectable(&commit.summary, cx), cx),
        row(
            "ID",
            selectable_mono(&commit.id.to_string(), mono.clone(), cx),
            cx,
        ),
    ];

    if let Some(parents) = parents_line(&commit.parents) {
        rows.push(row("Parents", selectable_mono(&parents, mono, cx), cx));
    }

    rows.push(row("Author", selectable(&author_line(commit), cx), cx));

    div().flex().flex_col().gap_1().p_3().children(rows)
}

/// The commit message body, if it has one beyond the subject line — `None` renders
/// nothing rather than an empty scroll-region row.
///
/// Unlike [`selectable`], the body is not escaped: it is prose the committer wrote, and
/// [`gpui_component::text::markdown`] rendering it as Markdown (lists, emphasis, code
/// spans) is the point, not a hazard to guard against.
pub(super) fn render_description(commit: &Commit, cx: &App) -> Option<AnyElement> {
    let body = commit.body.trim();
    if body.is_empty() {
        return None;
    }

    Some(
        div()
            .flex_shrink_0()
            .px_3()
            .py_3()
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(text::markdown(body.to_string()).selectable(true))
            .into_any_element(),
    )
}

fn selectable(value: &str, cx: &App) -> AnyElement {
    text::markdown(escape_markdown(value))
        .selectable(true)
        .text_color(cx.theme().foreground)
        .into_any_element()
}

fn selectable_mono(value: &str, mono: SharedString, cx: &App) -> AnyElement {
    text::markdown(escape_markdown(value))
        .selectable(true)
        .font_family(mono)
        .text_color(cx.theme().foreground)
        .into_any_element()
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
        .child(div().flex_1().min_w_0().child(value))
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
