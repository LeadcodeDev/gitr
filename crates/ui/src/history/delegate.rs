//! [`TableDelegate`] backing the history table: five columns over whichever commits
//! survive the current search query.
//!
//! Filtering happens here, over an already-loaded [`History`], using
//! [`HistoryFilter::matches`] rather than a reimplementation of it. Scope is not applied
//! here: the panel that owns this delegate only ever receives a `History` already walked
//! for the requested scope.

use std::sync::Arc;

use domain::{CommitSummary, ObjectId, Reference};
use gpui::{
    AnyElement, App, Bounds, Context, IntoElement, ParentElement as _, PathBuilder, Pixels,
    SharedString, Styled as _, Window, canvas, div, fill, point, px, size,
};
use gpui_component::{
    ActiveTheme as _, ThemeColor, h_flex,
    table::{Column, TableDelegate, TableState},
};
use graph::GraphRow;

use crate::graph_palette::lane_color;
use crate::repository::model::{History, HistoryFilter, LoadState};

use super::{badges, format, geometry};

const SHA_COLUMN: usize = 0;
const GRAPH_COLUMN: usize = 1;
const SUBJECT_COLUMN: usize = 2;
const AUTHOR_COLUMN: usize = 3;
const DATE_COLUMN: usize = 4;
const COLUMN_COUNT: usize = 5;

const SHA_COLUMN_WIDTH: Pixels = px(76.);
const SUBJECT_COLUMN_WIDTH: Pixels = px(420.);
const AUTHOR_COLUMN_WIDTH: Pixels = px(150.);
const DATE_COLUMN_WIDTH: Pixels = px(84.);

pub(crate) struct HistoryTableDelegate {
    history: LoadState<Arc<History>>,
    filter: HistoryFilter,
    visible_indices: Vec<usize>,
    graph_width: Pixels,
}

impl HistoryTableDelegate {
    pub(crate) fn new() -> Self {
        Self {
            history: LoadState::Idle,
            filter: HistoryFilter::default(),
            visible_indices: Vec::new(),
            graph_width: geometry::LANE_SPACING,
        }
    }

    pub(crate) fn set_history(&mut self, history: LoadState<Arc<History>>) {
        let graph_width = history
            .ready()
            .map(|loaded| geometry::gutter_width(loaded.layout.width, geometry::LANE_SPACING))
            .unwrap_or(geometry::LANE_SPACING);

        self.history = history;
        self.graph_width = graph_width;
        self.recompute_visible();
    }

    pub(crate) fn set_query(&mut self, query: String) {
        self.filter.query = query;
        self.recompute_visible();
    }

    pub(crate) fn commit_at(&self, row_ix: usize) -> Option<ObjectId> {
        let history = self.history.ready()?;
        let &commit_ix = self.visible_indices.get(row_ix)?;
        history.commits.get(commit_ix).map(|commit| commit.id)
    }

    fn recompute_visible(&mut self) {
        self.visible_indices.clear();
        let Some(history) = self.history.ready() else {
            return;
        };

        self.visible_indices.extend(
            history
                .commits
                .iter()
                .enumerate()
                .filter(|(_, commit)| self.filter.matches(commit))
                .map(|(index, _)| index),
        );
    }
}

impl TableDelegate for HistoryTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        COLUMN_COUNT
    }

    fn rows_count(&self, _: &App) -> usize {
        self.visible_indices.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        match col_ix {
            SHA_COLUMN => Column::new("sha", "SHA")
                .width(SHA_COLUMN_WIDTH)
                .min_width(SHA_COLUMN_WIDTH)
                .max_width(SHA_COLUMN_WIDTH)
                .resizable(false)
                .movable(false),
            GRAPH_COLUMN => Column::new("graph", "")
                .width(self.graph_width)
                .min_width(self.graph_width)
                .max_width(self.graph_width)
                .p_0()
                .resizable(false)
                .movable(false)
                .selectable(false),
            SUBJECT_COLUMN => Column::new("subject", "Subject").width(SUBJECT_COLUMN_WIDTH),
            AUTHOR_COLUMN => Column::new("author", "Author").width(AUTHOR_COLUMN_WIDTH),
            DATE_COLUMN => Column::new("date", "Date")
                .width(DATE_COLUMN_WIDTH)
                .text_right(),
            _ => Column::new("", ""),
        }
    }

    fn loading(&self, _: &App) -> bool {
        self.history.is_loading()
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let message: SharedString = match &self.history {
            LoadState::Idle | LoadState::Loading => "".into(),
            LoadState::Failed(message) => message.to_string().into(),
            LoadState::Ready(history) if history.is_empty() => "No commits.".into(),
            LoadState::Ready(_) => "No commit matches the current search.".into(),
        };

        h_flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(message)
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = cx.theme().colors;

        let Some(history) = self.history.ready() else {
            return div().into_any_element();
        };
        let Some(&commit_ix) = self.visible_indices.get(row_ix) else {
            return div().into_any_element();
        };
        let Some(commit) = history.commits.get(commit_ix) else {
            return div().into_any_element();
        };

        match col_ix {
            SHA_COLUMN => sha_cell(commit, &theme),
            GRAPH_COLUMN => match history.layout.rows.get(commit_ix) {
                Some(row) => graph_cell(row.clone(), &theme),
                None => div().into_any_element(),
            },
            SUBJECT_COLUMN => subject_cell(commit, history.references_at(commit.id), &theme),
            AUTHOR_COLUMN => author_cell(commit),
            DATE_COLUMN => date_cell(commit),
            _ => div().into_any_element(),
        }
    }
}

fn sha_cell(commit: &CommitSummary, theme: &ThemeColor) -> AnyElement {
    div()
        .h_full()
        .flex()
        .items_center()
        .px_2()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(format::abbreviate(commit.id))
        .into_any_element()
}

/// Paints one row's band of the gutter.
///
/// Takes no spacing argument on purpose. It once did, and the caller passed the gutter's
/// total width instead — the two are equal only for a single-lane history, so every
/// branchier repository silently placed lane 1 beyond the cell and clipped it away, node
/// and line together. There is exactly one correct value, so the parameter is gone.
fn graph_cell(row: GraphRow, theme: &ThemeColor) -> AnyElement {
    let theme = *theme;

    canvas(
        move |bounds, _, _| bounds,
        move |_, bounds, window, _| {
            let row_geometry =
                geometry::row_geometry(&row, bounds.size.height, geometry::LANE_SPACING);

            let strokes = row_geometry
                .incoming
                .iter()
                .chain(row_geometry.crossings.iter())
                .chain(row_geometry.outgoing.iter());

            for segment in strokes {
                let top = point(
                    bounds.origin.x + segment.top.x,
                    bounds.origin.y + segment.top.y,
                );
                let bottom = point(
                    bounds.origin.x + segment.bottom.x,
                    bounds.origin.y + segment.bottom.y,
                );
                let color = lane_color(segment.color, &theme);

                let mut builder = PathBuilder::stroke(geometry::LINE_WIDTH);
                builder.move_to(top);
                if segment.is_vertical {
                    builder.line_to(bottom);
                } else {
                    let mid_y = (top.y + bottom.y) * 0.5;
                    builder.cubic_bezier_to(bottom, point(top.x, mid_y), point(bottom.x, mid_y));
                }

                if let Ok(path) = builder.build() {
                    window.paint_path(path, color);
                }
            }

            let node_center = point(
                bounds.origin.x + row_geometry.node_center.x,
                bounds.origin.y + row_geometry.node_center.y,
            );
            let node_bounds = Bounds {
                origin: point(
                    node_center.x - geometry::NODE_RADIUS,
                    node_center.y - geometry::NODE_RADIUS,
                ),
                size: size(geometry::NODE_RADIUS * 2., geometry::NODE_RADIUS * 2.),
            };
            window.paint_quad(
                fill(node_bounds, lane_color(row_geometry.node_color, &theme))
                    .corner_radii(geometry::NODE_RADIUS),
            );
        },
    )
    .size_full()
    .into_any_element()
}

fn subject_cell(
    commit: &CommitSummary,
    references: &[Reference],
    theme: &ThemeColor,
) -> AnyElement {
    h_flex()
        .h_full()
        .items_center()
        .gap_1()
        .px_2()
        .overflow_hidden()
        .children(
            references
                .iter()
                .map(|reference| badges::render_badge(reference, theme).into_any_element()),
        )
        .child(div().truncate().child(commit.summary.clone()))
        .into_any_element()
}

fn author_cell(commit: &CommitSummary) -> AnyElement {
    div()
        .h_full()
        .flex()
        .items_center()
        .px_2()
        .text_sm()
        .truncate()
        .child(commit.author.name.clone())
        .into_any_element()
}

fn date_cell(commit: &CommitSummary) -> AnyElement {
    div()
        .h_full()
        .flex()
        .items_center()
        .justify_end()
        .px_2()
        .text_sm()
        .child(format::format_commit_date(&commit.author.time))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{BranchName, Parents, Signature, TagName, Timestamp};
    use graph::{GraphLayout, Lane, LaneColor, Segment};
    use std::collections::HashMap;

    fn id(nibble: char) -> ObjectId {
        nibble.to_string().repeat(40).parse().unwrap()
    }

    fn commit(id_nibble: char, summary: &str, author: &str, parents: Parents) -> CommitSummary {
        CommitSummary {
            id: id(id_nibble),
            parents,
            summary: summary.to_string(),
            author: Signature {
                name: author.to_string(),
                email: String::new(),
                time: Timestamp {
                    seconds: 0,
                    offset_minutes: 0,
                },
            },
        }
    }

    fn row(commit: char, lane: u16, color: u8, segments: Vec<Segment>) -> GraphRow {
        GraphRow {
            commit: id(commit),
            lane: Lane(lane),
            color: LaneColor(color),
            segments,
            has_incoming: false,
        }
    }

    fn fixture_history() -> History {
        let commits = vec![
            commit(
                '4',
                "merge: bring feature branch back into main",
                "B. P.",
                Parents::Merge(id('3'), id('2')),
            ),
            commit('3', "main progress", "B. P.", Parents::Linear(id('1'))),
            commit('2', "feature start", "A. Dev", Parents::Linear(id('1'))),
            commit('1', "root", "B. P.", Parents::Root),
        ];

        let rows = vec![
            row(
                '4',
                0,
                0,
                vec![
                    Segment {
                        from: Lane(0),
                        to: Lane(0),
                        color: LaneColor(0),
                    },
                    Segment {
                        from: Lane(0),
                        to: Lane(1),
                        color: LaneColor(1),
                    },
                ],
            ),
            row(
                '3',
                0,
                0,
                vec![
                    Segment {
                        from: Lane(0),
                        to: Lane(0),
                        color: LaneColor(0),
                    },
                    Segment {
                        from: Lane(1),
                        to: Lane(1),
                        color: LaneColor(1),
                    },
                ],
            ),
            row(
                '2',
                1,
                1,
                vec![
                    Segment {
                        from: Lane(0),
                        to: Lane(0),
                        color: LaneColor(0),
                    },
                    Segment {
                        from: Lane(1),
                        to: Lane(0),
                        color: LaneColor(1),
                    },
                ],
            ),
            row('1', 0, 0, Vec::new()),
        ];

        let mut refs_by_commit = HashMap::new();
        refs_by_commit.insert(
            id('4'),
            vec![Reference::LocalBranch(
                BranchName::new("chantier/m1-history-and-detail").unwrap(),
            )],
        );
        refs_by_commit.insert(
            id('3'),
            vec![
                Reference::LocalBranch(BranchName::new("main").unwrap()),
                Reference::RemoteBranch {
                    remote: domain::RemoteName::new("origin").unwrap(),
                    branch: BranchName::new("main").unwrap(),
                },
            ],
        );
        refs_by_commit.insert(
            id('1'),
            vec![Reference::Tag(TagName::new("v0.1.0").unwrap())],
        );

        History {
            commits,
            layout: GraphLayout { rows, width: 2 },
            refs_by_commit,
        }
    }

    #[test]
    fn an_empty_query_keeps_every_commit_in_order() {
        let mut delegate = HistoryTableDelegate::new();
        delegate.set_history(LoadState::Ready(Arc::new(fixture_history())));

        assert_eq!(delegate.visible_indices, vec![0, 1, 2, 3]);
        assert_eq!(delegate.commit_at(0), Some(id('4')));
        assert_eq!(delegate.commit_at(3), Some(id('1')));
    }

    #[test]
    fn a_query_narrows_the_visible_rows() {
        let mut delegate = HistoryTableDelegate::new();
        delegate.set_history(LoadState::Ready(Arc::new(fixture_history())));
        delegate.set_query("merge".to_string());

        assert_eq!(delegate.visible_indices, vec![0]);
        assert_eq!(delegate.commit_at(0), Some(id('4')));
        assert_eq!(delegate.commit_at(1), None);
    }

    #[test]
    fn a_query_matches_the_author_name() {
        let mut delegate = HistoryTableDelegate::new();
        delegate.set_history(LoadState::Ready(Arc::new(fixture_history())));
        delegate.set_query("dev".to_string());

        assert_eq!(delegate.visible_indices, vec![2]);
    }

    #[test]
    fn clearing_the_query_restores_every_row() {
        let mut delegate = HistoryTableDelegate::new();
        delegate.set_history(LoadState::Ready(Arc::new(fixture_history())));
        delegate.set_query("merge".to_string());
        delegate.set_query(String::new());

        assert_eq!(delegate.visible_indices, vec![0, 1, 2, 3]);
    }

    #[test]
    fn the_graph_width_covers_every_lane_the_layout_uses() {
        let mut delegate = HistoryTableDelegate::new();
        delegate.set_history(LoadState::Ready(Arc::new(fixture_history())));

        assert_eq!(delegate.graph_width, geometry::LANE_SPACING * 2usize);
    }

    #[test]
    fn an_unloaded_history_has_no_visible_rows() {
        let delegate = HistoryTableDelegate::new();
        assert!(delegate.visible_indices.is_empty());
        assert_eq!(delegate.commit_at(0), None);
    }
}
