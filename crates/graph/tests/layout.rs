//! Every assertion here reads the layout the way GitX draws a cell: `incoming` is the
//! lines crossing the upper half into the row's centre, `segments` the lines crossing the
//! lower half out of it. A change of column happens in the upper half, so a line arrives
//! at the centre already in its new one.

use std::str::FromStr;

use domain::{CommitSummary, HEX_LEN, ObjectId, Parents, Signature, Timestamp};
use graph::layout::layout;
use graph::{Lane, LaneColor, Segment};

fn oid(label: &str) -> ObjectId {
    let mut hex = String::with_capacity(HEX_LEN);
    for byte in label.bytes() {
        hex.push_str(&format!("{byte:02x}"));
    }
    while hex.len() < HEX_LEN {
        hex.push('0');
    }
    hex.truncate(HEX_LEN);
    ObjectId::from_str(&hex).unwrap()
}

fn commit(label: &str, parents: Parents) -> CommitSummary {
    CommitSummary {
        id: oid(label),
        parents,
        summary: label.to_string(),
        author: Signature {
            name: "author".to_string(),
            email: "author@example.com".to_string(),
            time: Timestamp {
                seconds: 0,
                offset_minutes: 0,
            },
        },
    }
}

fn seg(from: u16, to: u16, color: u8) -> Segment {
    Segment {
        from: Lane(from),
        to: Lane(to),
        color: LaneColor(color),
    }
}

#[test]
fn empty_input_produces_an_empty_layout() {
    let result = layout(&[]);
    assert!(result.rows.is_empty());
    assert_eq!(result.width, 0);
}

#[test]
fn a_single_root_commit_takes_lane_zero_and_draws_nothing_below() {
    let result = layout(&[commit("only", Parents::Root)]);

    assert_eq!(result.rows[0].lane, Lane(0));
    assert!(result.rows[0].incoming.is_empty());
    assert!(
        result.rows[0].segments.is_empty(),
        "a commit with no parents has nothing to draw below it"
    );
    assert_eq!(result.width, 1);
}

#[test]
fn a_linear_history_never_leaves_lane_zero() {
    let commits = [
        commit("c", Parents::Linear(oid("b"))),
        commit("b", Parents::Linear(oid("a"))),
        commit("a", Parents::Root),
    ];

    let result = layout(&commits);

    for row in &result.rows {
        assert_eq!(row.lane, Lane(0));
    }
    assert_eq!(result.rows[0].segments, vec![seg(0, 0, 0)]);
    assert_eq!(result.rows[1].incoming, vec![seg(0, 0, 0)]);
    assert_eq!(result.rows[1].segments, vec![seg(0, 0, 0)]);
    assert!(result.rows[2].segments.is_empty());
    assert_eq!(result.width, 1);
}

#[test]
fn a_branch_tip_has_nothing_reaching_its_own_column_from_above() {
    let commits = [
        commit("main2", Parents::Linear(oid("main1"))),
        commit("feature1", Parents::Linear(oid("base"))),
        commit("main1", Parents::Linear(oid("base"))),
        commit("base", Parents::Root),
    ];

    let result = layout(&commits);
    let tip = &result.rows[1];

    assert_eq!(tip.lane, Lane(1));
    assert!(
        !tip.has_incoming(),
        "nothing points at a tip, so no line may reach its column from above"
    );
    assert_eq!(
        tip.incoming,
        vec![seg(0, 0, 0)],
        "the trunk still crosses the row it appears in"
    );
}

#[test]
fn two_columns_waiting_for_the_same_commit_meet_only_where_it_is_placed() {
    let commits = [
        commit("main2", Parents::Linear(oid("main1"))),
        commit("feature1", Parents::Linear(oid("base"))),
        commit("main1", Parents::Linear(oid("base"))),
        commit("base", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(
        result.rows[2].segments,
        vec![seg(0, 0, 0), seg(1, 1, 1)],
        "both columns still hold base here, so both run straight down"
    );
    assert_eq!(
        result.rows[3].incoming,
        vec![seg(0, 0, 0), seg(1, 0, 1)],
        "and both reach its node in the row that places it, each keeping its own colour"
    );
    assert_eq!(result.width, 2);
}

#[test]
fn a_merge_second_parent_leaves_the_node_sideways() {
    let commits = [
        commit("main2", Parents::Linear(oid("main1"))),
        commit("feature", Parents::Linear(oid("base"))),
        commit("main1", Parents::Merge(oid("base"), oid("feature"))),
        commit("base", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(
        result.rows[2].segments,
        vec![seg(0, 0, 0), seg(1, 1, 1), seg(0, 2, 2)],
        "the second parent starts at the merge's own column, not at the one it lands in"
    );
    assert_eq!(result.width, 3);
}

#[test]
fn an_octopus_merge_opens_one_column_per_parent() {
    let commits = [
        commit(
            "merge",
            Parents::Octopus(vec![oid("p0"), oid("p1"), oid("p2")]),
        ),
        commit("p0", Parents::Root),
        commit("p1", Parents::Root),
        commit("p2", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(
        result.rows[0].segments,
        vec![seg(0, 0, 0), seg(0, 1, 1), seg(0, 2, 2)],
        "every parent leaves the merge's own node"
    );
    assert_eq!(result.width, 3);
}

#[test]
fn a_column_freed_by_a_root_is_reused_without_growing_the_width() {
    let commits = [
        commit("long3", Parents::Linear(oid("long2"))),
        commit("short", Parents::Root),
        commit("long2", Parents::Linear(oid("long1"))),
        commit("later", Parents::Root),
        commit("long1", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(
        result.rows[1].lane,
        Lane(1),
        "the first short root opens one"
    );
    assert!(
        !result.rows[1]
            .segments
            .iter()
            .any(|segment| segment.from == Lane(1)),
        "and closes it again, having no parents, while the trunk still crosses the row"
    );
    assert_eq!(
        result.rows[3].lane,
        Lane(1),
        "the next unrelated root takes the freed column rather than a new one"
    );
    assert_eq!(result.width, 2);
}

#[test]
fn width_reflects_the_peak_column_count_not_the_final_row() {
    let commits = [
        commit("merge", Parents::Merge(oid("p0"), oid("p1"))),
        commit("p1", Parents::Linear(oid("p0"))),
        commit("p0", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.width, 2);
    assert!(
        result.rows.last().expect("a row").segments.is_empty(),
        "the last row narrows back to nothing, which must not narrow the gutter"
    );
}

#[test]
fn colours_follow_a_track_rather_than_a_column() {
    let commits = [
        commit("main2", Parents::Linear(oid("main1"))),
        commit("feature1", Parents::Linear(oid("base"))),
        commit("main1", Parents::Linear(oid("base"))),
        commit("base", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.rows[0].color, LaneColor(0));
    assert_eq!(
        result.rows[1].color,
        LaneColor(1),
        "a newly opened track takes the next palette slot"
    );
    assert_eq!(
        result.rows[2].color,
        LaneColor(0),
        "and the trunk keeps its own across the rows the branch occupies"
    );
}
