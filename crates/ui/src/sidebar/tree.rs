//! A dense, purpose-built row for the sidebar's section and reference tree.
//!
//! [`gpui_component::sidebar::SidebarMenuItem`] hard-codes `h_7()` and `text_sm()` for
//! every row regardless of depth, always reserves label space for a caret only on nodes
//! that already have children (misaligning leaves against their siblings), and never
//! stops its label from wrapping. None of that meets what this sidebar's reference tree
//! needs: an exact row height, a fixed indent per nesting depth, an icon slot that only
//! the top level uses, and a label that ellipsizes instead of wrapping. [`SidebarTreeItem`]
//! is a from-scratch replacement scoped to exactly that; [`super::SidebarHeader`] (the
//! repository selector) is unaffected and stays on the vendor component.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, percentage, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Collapsible, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    sidebar::SidebarItem,
    v_flex,
};

use crate::density;

/// Vertical gap separating one top-level section (`Working`, `Branches`, ...) from the
/// next. [`gpui_component::sidebar::Sidebar`] lays its content out through a virtualised
/// `list`, which does not itself space consecutive items apart.
const SECTION_GAP: Pixels = px(4.);

/// Fixed horizontal padding on every row, independent of [`density::SIDEBAR_INDENT`]
/// depth indentation.
const ROW_PADDING_X: Pixels = px(6.);

/// Size of the caret slot reserved on every row, whether or not it has children — this is
/// what keeps a leaf's label aligned with its siblings' labels rather than starting
/// further left than them. Matches the footprint of the `Button::xsmall()` icon button
/// used for the caret itself (`Size::XSmall`'s icon-only square, `size_5()`) so a leaf
/// and a parent at the same depth line their labels up exactly.
const DISCLOSURE_SIZE: Pixels = px(15.);

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One row of the sidebar's section and reference tree.
///
/// Used both for the five top-level sections (`Working`, `Branches`, `Remotes`, `Tags`,
/// `Stashes`) and for every nested branch, tag and remote beneath them — the only
/// difference is depth, tracked internally as this recurses, and that only depth 0 ever
/// shows [`Self::icon`].
#[derive(Clone)]
pub struct SidebarTreeItem {
    icon: Option<IconName>,
    label: SharedString,
    active: bool,
    default_open: bool,
    on_click: Option<ClickHandler>,
    children: Vec<SidebarTreeItem>,
    sidebar_collapsed: bool,
}

impl SidebarTreeItem {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            icon: None,
            label: label.into(),
            active: false,
            default_open: false,
            on_click: None,
            children: Vec::new(),
            sidebar_collapsed: false,
        }
    }

    /// Only rendered at depth 0 — see the module-level rationale for dropping icons on
    /// nested rows.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The one piece of state this tree must never lose: which reference is checked out.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
}

impl Collapsible for SidebarTreeItem {
    fn is_collapsed(&self) -> bool {
        self.sidebar_collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.sidebar_collapsed = collapsed;
        self
    }
}

impl SidebarItem for SidebarTreeItem {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let sidebar_collapsed = self.sidebar_collapsed;
        render_node(self, id.into(), 0, sidebar_collapsed, window, cx)
    }
}

/// Renders `item` and, if open, its subtree.
///
/// `sidebar_collapsed` is the *sidebar's* icon-rail state (set once, from
/// [`Collapsible::collapsed`]), not this node's own open/closed state — an icon-rail
/// sidebar shows only depth-0 icons and never descends into children, matching
/// `SidebarMenuItem`'s behaviour.
fn render_node(
    item: SidebarTreeItem,
    id: ElementId,
    depth: usize,
    sidebar_collapsed: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if sidebar_collapsed && depth == 0 {
        return render_collapsed_top_level(&item, cx).into_any_element();
    }

    let has_children = !item.children.is_empty();
    let open_state =
        has_children.then(|| window.use_keyed_state(id.clone(), cx, |_, _| item.default_open));
    let is_open = open_state.as_ref().is_some_and(|state| *state.read(cx));

    let row = render_row(&item, depth, open_state, is_open, cx);
    let mut node = v_flex().id(id.clone()).w_full().child(row);

    if is_open {
        node = node.child(v_flex().id("children").w_full().children(
            item.children.into_iter().enumerate().map(|(ix, child)| {
                let child_id = ElementId::from(SharedString::from(format!("{id}-{ix}")));
                render_node(child, child_id, depth + 1, sidebar_collapsed, window, cx)
            }),
        ));
    }

    if depth == 0 {
        node = node.mb(SECTION_GAP);
    }

    node.into_any_element()
}

fn render_row(
    item: &SidebarTreeItem,
    depth: usize,
    open_state: Option<Entity<bool>>,
    is_open: bool,
    cx: &mut App,
) -> impl IntoElement {
    let indent = density::SIDEBAR_INDENT * (depth as f32);
    let is_top_level = depth == 0;

    let disclosure: AnyElement = match open_state {
        Some(open_state) => Button::new("disclosure")
            .ghost()
            .xsmall()
            .icon(
                Icon::new(IconName::ChevronRight)
                    .size_4()
                    .when(is_open, |this| this.rotate(percentage(90. / 360.))),
            )
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                open_state.update(cx, |open, cx| {
                    *open = !*open;
                    cx.notify();
                });
            })
            .into_any_element(),
        None => div()
            .size(DISCLOSURE_SIZE)
            .flex_shrink_0()
            .into_any_element(),
    };

    let handler = item.on_click.clone();

    h_flex()
        .id("row")
        .w_full()
        .h(density::SIDEBAR_ROW_HEIGHT)
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .pl(indent + ROW_PADDING_X)
        .pr(ROW_PADDING_X)
        .rounded(cx.theme().radius)
        .when(item.active, |this| {
            this.bg(cx.theme().tokens.sidebar_accent)
                .text_color(cx.theme().sidebar_accent_foreground)
                .font_medium()
        })
        .when(!item.active, |this| {
            this.hover(|this| {
                this.bg(cx.theme().sidebar_accent.opacity(0.8))
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
        })
        .child(disclosure)
        .when_some(item.icon.clone().filter(|_| is_top_level), |this, icon| {
            this.child(Icon::new(icon).size_4())
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(item.label.clone()),
        )
        .when_some(handler, |this, handler| {
            this.on_click(move |ev, window, cx| handler(ev, window, cx))
        })
}

/// The icon-only presentation used for depth-0 items when the sidebar itself is
/// collapsed to its icon rail. Nested rows never reach this: a collapsed sidebar renders
/// no children at all, matching `SidebarMenuItem`. A `Button` rather than a plain `div`
/// because `ManagedTooltipExt`, the element-level tooltip trait, is private to
/// gpui-component — `Button::tooltip` is the public surface for the same behaviour.
fn render_collapsed_top_level(item: &SidebarTreeItem, cx: &App) -> impl IntoElement {
    div()
        .id("row")
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .py_1()
        .child(
            Button::new("icon")
                .ghost()
                .when_some(item.icon.clone(), |this, icon| {
                    this.icon(Icon::new(icon).size_4())
                        .tooltip(item.label.clone())
                })
                .when(item.active, |this| {
                    this.bg(cx.theme().tokens.sidebar_accent)
                        .text_color(cx.theme().sidebar_accent_foreground)
                }),
        )
}
