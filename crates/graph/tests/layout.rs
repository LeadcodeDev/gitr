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
    let commits = [commit("root", Parents::Root)];

    let result = layout(&commits);

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].lane, Lane(0));
    assert_eq!(result.rows[0].color, LaneColor(0));
    assert!(result.rows[0].segments.is_empty());
    assert_eq!(result.width, 1);
}

#[test]
fn a_linear_history_never_leaves_lane_zero() {
    let commits = [
        commit("c3", Parents::Linear(oid("c2"))),
        commit("c2", Parents::Linear(oid("c1"))),
        commit("c1", Parents::Linear(oid("root"))),
        commit("root", Parents::Root),
    ];

    let result = layout(&commits);

    for row in &result.rows[..3] {
        assert_eq!(row.lane, Lane(0));
        assert_eq!(row.color, LaneColor(0));
        assert_eq!(row.segments, vec![seg(0, 0, 0)]);
    }
    assert_eq!(result.rows[3].lane, Lane(0));
    assert!(result.rows[3].segments.is_empty());
    assert_eq!(result.width, 1);
}

#[test]
fn a_branch_that_diverges_and_never_rejoins_keeps_its_own_lane_until_the_shared_root() {
    let commits = [
        commit("main2", Parents::Linear(oid("main1"))),
        commit("feature1", Parents::Linear(oid("base"))),
        commit("main1", Parents::Linear(oid("base"))),
        commit("base", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.rows[0].lane, Lane(0));
    assert_eq!(result.rows[0].color, LaneColor(0));
    assert_eq!(result.rows[0].segments, vec![seg(0, 0, 0)]);

    assert_eq!(result.rows[1].lane, Lane(1));
    assert_eq!(result.rows[1].color, LaneColor(1));
    assert_eq!(result.rows[1].segments, vec![seg(0, 0, 0), seg(1, 1, 1)]);

    assert_eq!(result.rows[2].lane, Lane(0));
    assert_eq!(result.rows[2].color, LaneColor(0));
    assert_eq!(result.rows[2].segments, vec![seg(0, 0, 0), seg(1, 0, 0)]);

    assert_eq!(result.rows[3].lane, Lane(0));
    assert_eq!(result.rows[3].color, LaneColor(0));
    assert!(result.rows[3].segments.is_empty());

    assert_eq!(result.width, 2);
}

#[test]
fn a_branch_that_diverges_and_merges_back_frees_its_lane_again() {
    let commits = [
        commit("merge", Parents::Merge(oid("main1"), oid("feature1"))),
        commit("feature1", Parents::Linear(oid("base"))),
        commit("main1", Parents::Linear(oid("base"))),
        commit("base", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.rows[0].lane, Lane(0));
    assert_eq!(result.rows[0].color, LaneColor(0));
    assert_eq!(result.rows[0].segments, vec![seg(0, 0, 0), seg(0, 1, 1)]);

    assert_eq!(result.rows[1].lane, Lane(1));
    assert_eq!(result.rows[1].color, LaneColor(1));
    assert_eq!(result.rows[1].segments, vec![seg(0, 0, 0), seg(1, 1, 1)]);

    assert_eq!(result.rows[2].lane, Lane(0));
    assert_eq!(result.rows[2].color, LaneColor(0));
    assert_eq!(result.rows[2].segments, vec![seg(0, 0, 0), seg(1, 0, 0)]);

    assert_eq!(result.rows[3].lane, Lane(0));
    assert_eq!(result.rows[3].color, LaneColor(0));
    assert!(result.rows[3].segments.is_empty());

    assert_eq!(result.width, 2);
}

#[test]
fn two_independent_roots_each_get_their_own_lane() {
    let commits = [
        commit("a2", Parents::Linear(oid("a1"))),
        commit("b2", Parents::Linear(oid("b1"))),
        commit("a1", Parents::Root),
        commit("b1", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.rows[0].lane, Lane(0));
    assert_eq!(result.rows[0].segments, vec![seg(0, 0, 0)]);

    assert_eq!(result.rows[1].lane, Lane(1));
    assert_eq!(result.rows[1].segments, vec![seg(0, 0, 0), seg(1, 1, 1)]);

    assert_eq!(result.rows[2].lane, Lane(0));
    assert_eq!(result.rows[2].segments, vec![seg(1, 0, 1)]);

    assert_eq!(result.rows[3].lane, Lane(0));
    assert!(result.rows[3].segments.is_empty());

    assert_eq!(result.width, 2);
}

#[test]
fn an_octopus_merge_opens_one_lane_per_parent() {
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

    assert_eq!(result.rows[0].lane, Lane(0));
    assert_eq!(result.rows[0].color, LaneColor(0));
    assert_eq!(
        result.rows[0].segments,
        vec![seg(0, 0, 0), seg(0, 1, 1), seg(0, 2, 2)]
    );

    assert_eq!(result.rows[1].lane, Lane(0));
    assert_eq!(result.rows[1].segments, vec![seg(1, 0, 1), seg(2, 1, 2)]);

    assert_eq!(result.rows[2].lane, Lane(0));
    assert_eq!(result.rows[2].segments, vec![seg(1, 0, 2)]);

    assert_eq!(result.rows[3].lane, Lane(0));
    assert!(result.rows[3].segments.is_empty());

    assert_eq!(result.width, 3);
}

#[test]
fn a_freed_lane_is_reused_by_a_later_unrelated_branch_without_growing_width() {
    let commits = [
        commit("long3", Parents::Linear(oid("long2"))),
        commit("short_tip", Parents::Linear(oid("short_root"))),
        commit("short_root", Parents::Root),
        commit("late_tip", Parents::Linear(oid("late_root"))),
        commit("long2", Parents::Linear(oid("long1"))),
        commit("late_root", Parents::Root),
        commit("long1", Parents::Linear(oid("long0"))),
        commit("long0", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.rows[0].lane, Lane(0));
    assert_eq!(result.rows[0].color, LaneColor(0));

    assert_eq!(result.rows[1].lane, Lane(1));
    assert_eq!(result.rows[1].color, LaneColor(1));

    assert_eq!(result.rows[2].lane, Lane(1));
    assert_eq!(result.rows[2].segments, vec![seg(0, 0, 0)]);

    assert_eq!(result.rows[3].lane, Lane(1));
    assert_eq!(result.rows[3].color, LaneColor(2));
    assert_eq!(result.rows[3].segments, vec![seg(0, 0, 0), seg(1, 1, 2)]);

    assert_eq!(result.rows[4].lane, Lane(0));
    assert_eq!(result.rows[5].lane, Lane(1));
    assert_eq!(result.rows[5].color, LaneColor(2));
    assert_eq!(result.rows[6].lane, Lane(0));
    assert_eq!(result.rows[7].lane, Lane(0));

    assert_eq!(result.width, 2);
}

#[test]
fn width_reflects_the_peak_lane_count_not_the_final_row() {
    let commits = [
        commit("merge", Parents::Merge(oid("p0"), oid("p1"))),
        commit("p0", Parents::Linear(oid("shared"))),
        commit("p1", Parents::Linear(oid("shared"))),
        commit("shared", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(result.rows.len(), 4);
    assert_eq!(result.rows[3].lane, Lane(0));
    assert!(result.rows[3].segments.is_empty());
    assert_eq!(result.width, 2);
    assert_ne!(result.width as usize, result.rows.len());
}
