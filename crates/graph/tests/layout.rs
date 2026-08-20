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
    assert_eq!(
        result.rows[2].segments,
        vec![seg(0, 0, 0), seg(1, 0, 1)],
        "the branch keeps its own colour on the segment that reaches the track it rejoins, \
         or it appears to end a row before it does"
    );

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
    assert_eq!(
        result.rows[2].segments,
        vec![seg(0, 0, 0), seg(1, 0, 1)],
        "the merged branch stays its own colour all the way into the junction"
    );

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

#[test]
fn a_tip_has_nothing_incoming_while_everything_it_reaches_does() {
    let commits = vec![
        commit("c", Parents::Linear(oid("b"))),
        commit("b", Parents::Linear(oid("a"))),
        commit("a", Parents::Root),
    ];

    let layout = layout(&commits);

    assert!(
        !layout.rows[0].has_incoming(),
        "the newest commit has no child above it"
    );
    assert!(layout.rows[1].has_incoming());
    assert!(layout.rows[2].has_incoming());
}

#[test]
fn each_branch_tip_starts_its_own_track_with_nothing_above_it() {
    let commits = vec![
        commit("t1", Parents::Linear(oid("root"))),
        commit("t2", Parents::Linear(oid("root"))),
        commit("root", Parents::Root),
    ];

    let layout = layout(&commits);

    assert!(!layout.rows[0].has_incoming());
    assert!(
        !layout.rows[1].has_incoming(),
        "a second tip opens a fresh lane, so nothing reaches it from above either"
    );
    assert!(
        layout.rows[2].has_incoming(),
        "both tips point at the root, so a line does reach it"
    );
}

#[test]
fn a_root_commit_emits_no_segment_of_its_own() {
    let commits = vec![commit("only", Parents::Root)];

    let layout = layout(&commits);

    assert!(
        layout.rows[0].segments.is_empty(),
        "nothing continues below a root, so its band draws nothing downward"
    );
    assert!(!layout.rows[0].has_incoming());
}

/// GitX defers convergence: a commit's first parent takes over its lane without checking
/// whether another column already expects that parent, so two columns hold the same object
/// until the row that places it. Converging eagerly instead closes a column a row early and
/// slides everything to its right, which is what made ten of this repository's own commits
/// sit one column left of where GitX draws them.
#[test]
fn a_branch_keeps_its_column_until_the_shared_parent_is_actually_reached() {
    let commits = [
        commit("main2", Parents::Linear(oid("main1"))),
        commit("side", Parents::Linear(oid("main1"))),
        commit("main1", Parents::Root),
    ];

    let result = layout(&commits);

    assert_eq!(
        result.rows[1].lane,
        Lane(1),
        "the branch tip opens a column"
    );
    assert_eq!(
        result.rows[1].segments,
        vec![seg(0, 0, 0), seg(1, 0, 1)],
        "both columns still hold main1 through this band, and meet only where it is placed"
    );
    assert_eq!(result.rows[2].lane, Lane(0));
    assert_eq!(
        result.rows[2].incoming.len(),
        2,
        "the shared parent receives a line from each column that was waiting for it"
    );
    assert_eq!(result.width, 2);
}
