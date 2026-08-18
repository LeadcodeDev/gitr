use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div};

/// Root view of a gitr window.
pub struct Workspace;

impl Workspace {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("gitr")
    }
}
