//! [`HistoryPanel`]: the centre-dock view over a loaded [`History`].
//!
//! Renders whatever [`LoadState`] it is given — it never reads a repository itself. A
//! scope tab bar and a search box sit above a virtualised [`DataTable`]; selecting a row
//! or changing either control reports back through [`HistoryPanelEvent`] rather than
//! acting on the repository directly, which is a sibling's job.

use std::sync::Arc;

use domain::{HistoryScope, ObjectId, Reference};
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    dock::{Panel, PanelEvent},
    h_flex,
    input::{Input, InputEvent, InputState},
    tab::{Tab, TabBar},
    table::{DataTable, TableEvent, TableState},
    v_flex,
};

use crate::density;
use crate::repository::model::{History, HistoryFilter, LoadState};

use super::delegate::HistoryTableDelegate;

/// Reported by [`HistoryPanel`] when the user acts on it.
///
/// Never emitted for a change [`HistoryPanel::set_history`] or
/// [`HistoryPanel::set_filter`] itself caused — those are inbound state, and echoing them
/// back out would loop.
pub enum HistoryPanelEvent {
    Selected(ObjectId),
    FilterChanged(HistoryFilter),
}

impl EventEmitter<HistoryPanelEvent> for HistoryPanel {}
impl EventEmitter<PanelEvent> for HistoryPanel {}

/// The scopes a [`HistoryPanel`] offers, given the branch its filter is currently
/// (or was last) scoped to.
///
/// Pure so the tab set is testable without a window: the panel never invents a branch to
/// scope by, it only ever offers `All`, `Local`, and whichever single reference it was
/// last handed through [`HistoryFilter::scope`].
fn scope_options(branch_scope: &Option<Reference>) -> Vec<HistoryScope> {
    let mut scopes = vec![HistoryScope::AllReferences, HistoryScope::LocalReferences];
    if let Some(reference) = branch_scope {
        scopes.push(HistoryScope::Single(reference.clone()));
    }
    scopes
}

/// The tab label for one scope.
fn scope_label(scope: &HistoryScope) -> String {
    match scope {
        HistoryScope::AllReferences => "Tous".to_string(),
        HistoryScope::LocalReferences => "Local".to_string(),
        HistoryScope::Single(reference) => reference.short_name(),
    }
}

pub struct HistoryPanel {
    table: Entity<TableState<HistoryTableDelegate>>,
    search_input: Entity<InputState>,
    filter: HistoryFilter,
    branch_scope: Option<Reference>,
    selected: Option<ObjectId>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl HistoryPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = HistoryTableDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx).sortable(false));
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));

        let table_subscription = cx.subscribe_in(&table, window, Self::on_table_event);
        let search_subscription = cx.subscribe_in(&search_input, window, Self::on_search_event);

        Self {
            table,
            search_input,
            filter: HistoryFilter::default(),
            branch_scope: None,
            selected: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![table_subscription, search_subscription],
        }
    }

    pub fn set_history(&mut self, history: LoadState<Arc<History>>, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            table.delegate_mut().set_history(history);
            table.refresh(cx);
        });
        cx.notify();
    }

    pub fn set_filter(&mut self, filter: HistoryFilter, cx: &mut Context<Self>) {
        if let HistoryScope::Single(reference) = &filter.scope {
            self.branch_scope = Some(reference.clone());
        }

        let query_changed = filter.query != self.filter.query;
        self.filter = filter;

        if query_changed {
            let query = self.filter.query.clone();
            self.table.update(cx, |table, cx| {
                table.delegate_mut().set_query(query);
                table.refresh(cx);
            });
        }

        cx.notify();
    }

    pub fn selected(&self) -> Option<ObjectId> {
        self.selected
    }

    fn select_scope(&mut self, scope: HistoryScope, cx: &mut Context<Self>) {
        self.filter.scope = scope;
        cx.emit(HistoryPanelEvent::FilterChanged(self.filter.clone()));
        cx.notify();
    }

    fn on_table_event(
        &mut self,
        table: &Entity<TableState<HistoryTableDelegate>>,
        event: &TableEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TableEvent::SelectRow(row_ix) = event else {
            return;
        };

        let Some(commit) = table.read(cx).delegate().commit_at(*row_ix) else {
            return;
        };

        self.selected = Some(commit);
        cx.emit(HistoryPanelEvent::Selected(commit));
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let InputEvent::Change = event else {
            return;
        };

        let query = input.read(cx).value().to_string();
        self.filter.query = query.clone();
        self.table.update(cx, |table, cx| {
            table.delegate_mut().set_query(query);
            table.refresh(cx);
        });
        cx.emit(HistoryPanelEvent::FilterChanged(self.filter.clone()));
        cx.notify();
    }
}

impl Panel for HistoryPanel {
    fn panel_name(&self) -> &'static str {
        "HistoryPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "History"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Focusable for HistoryPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HistoryPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_query = self.search_input.read(cx).value().to_string();
        if current_query != self.filter.query {
            let query = self.filter.query.clone();
            self.search_input.update(cx, |input, cx| {
                input.set_value(query, window, cx);
            });
        }

        let scopes = scope_options(&self.branch_scope);
        let selected_index = scopes.iter().position(|scope| scope == &self.filter.scope);

        let mut tabs = TabBar::new("history-scope")
            .segmented()
            .small()
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                if let Some(scope) = scope_options(&this.branch_scope).get(*index).cloned() {
                    this.select_scope(scope, cx);
                }
            }));
        if let Some(index) = selected_index {
            tabs = tabs.selected_index(index);
        }
        for scope in &scopes {
            tabs = tabs.child(Tab::new().label(scope_label(scope)));
        }

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(tabs)
                    .child(
                        Input::new(&self.search_input)
                            .small()
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).small())
                            .w(px(220.)),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    DataTable::new(&self.table)
                        .stripe(true)
                        .bordered(false)
                        .with_size(density::TABLE_ROW_HEIGHT),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::BranchName;

    #[test]
    fn without_a_branch_scope_only_all_and_local_are_offered() {
        let scopes = scope_options(&None);
        assert_eq!(
            scopes,
            vec![HistoryScope::AllReferences, HistoryScope::LocalReferences]
        );
    }

    #[test]
    fn a_remembered_branch_becomes_a_third_option() {
        let reference = Reference::LocalBranch(BranchName::new("main").unwrap());
        let scopes = scope_options(&Some(reference.clone()));
        assert_eq!(
            scopes,
            vec![
                HistoryScope::AllReferences,
                HistoryScope::LocalReferences,
                HistoryScope::Single(reference),
            ]
        );
    }

    #[test]
    fn scope_labels_read_in_french_for_the_built_in_scopes() {
        assert_eq!(scope_label(&HistoryScope::AllReferences), "Tous");
        assert_eq!(scope_label(&HistoryScope::LocalReferences), "Local");
    }

    #[test]
    fn a_single_scope_labels_itself_with_the_references_short_name() {
        let reference = Reference::LocalBranch(BranchName::new("chantier/m1").unwrap());
        assert_eq!(scope_label(&HistoryScope::Single(reference)), "chantier/m1");
    }
}
