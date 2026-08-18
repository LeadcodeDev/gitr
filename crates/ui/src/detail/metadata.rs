//! Renders the commit metadata header: subject, identifier, parents and author, followed
//! by the commit message body when there is one.

use domain::{Commit, Parents};
use gpui::{
    AnyElement, App, IntoElement, ParentElement as _, Styled as _, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::ActiveTheme as _;

use super::format::{abbreviate, format_timestamp};

const LABEL_WIDTH: f32 = 84.;

pub(super) fn render(commit: &Commit, cx: &App) -> impl IntoElement {
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

    div().flex().flex_col().gap_1().p_3().children(rows).when(
        !commit.body.trim().is_empty(),
        |element| {
            element.child(
                div()
                    .mt_2()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(commit.body.trim().to_string()),
            )
        },
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
