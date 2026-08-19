//! Track placement: assigns each commit a lane and the segments its row draws.
//!
//! The model is git's own `graph.c`: a lane is a reservation holding the [`ObjectId`] of
//! the next commit expected in that column. Walking the input in order, a commit is placed
//! into the column that already expects it, or into a freshly opened one if no column does.
//! Its parents then take over columns for the rows below: a parent that some other column
//! already expects converges onto that column instead of opening a new one, which is what
//! makes two diverging tracks reconverge onto a shared ancestor. A column whose commit has
//! no parents (`Parents::Root`) is dropped rather than carried forward, which frees the lane
//! for a later, unrelated track to reuse.

use std::collections::HashMap;

use domain::{CommitSummary, ObjectId};

use crate::model::{GraphLayout, GraphRow, Lane, LaneColor, PALETTE_SIZE, Segment};

/// A column's current reservation: the commit it expects next, and the colour that column
/// has carried since the track using it was opened.
#[derive(Clone, Copy)]
struct Track {
    commit: ObjectId,
    color: LaneColor,
}

/// Assigns a lane to every commit and records, per row, the segments the row's band draws.
///
/// `commits` must already be topologically ordered with date priority: a commit appears
/// before every one of its ancestors. Placement does not verify this and produces a
/// misleading layout if it is violated — the crate-level docs explain why order is not
/// this function's job.
///
/// Runs in `O(commits.len() * peak_width)` with a `HashMap` lookup per column visited per
/// row, since each row rebuilds its column list from the one above by scanning it once.
///
/// A segment is drawn in the colour of the track it *continues*, never the one it lands
/// on. A commit's first-parent link continues its own track, so it keeps the commit's
/// colour even where it converges into a lane another track already holds — otherwise a
/// branch runs in its own colour for its whole length and then changes colour on the one
/// segment that reaches the branch it rejoins, which reads as the branch ending a row
/// early. A second-parent link is the other case: it does not continue the merge commit's
/// track, it reaches into the track being merged, so it takes that track's colour.
pub fn layout(commits: &[CommitSummary]) -> GraphLayout {
    let mut tracks: Vec<Track> = Vec::new();
    let mut track_lane: HashMap<ObjectId, usize> = HashMap::new();
    let mut rows = Vec::with_capacity(commits.len());
    let mut width: u16 = 0;
    let mut palette_cursor: u8 = 0;

    for commit in commits {
        let was_reserved = track_lane.contains_key(&commit.id);
        let lane_index = place(commit.id, &mut tracks, &mut track_lane, || {
            allocate_color(&mut palette_cursor)
        });
        let commit_color = tracks[lane_index].color;

        let mut next_tracks: Vec<Track> = Vec::with_capacity(tracks.len());
        let mut next_lane: HashMap<ObjectId, usize> = HashMap::with_capacity(tracks.len());
        let mut segments: Vec<Segment> = Vec::with_capacity(tracks.len());

        for (index, track) in tracks.iter().enumerate() {
            if index == lane_index {
                for (parent_index, parent) in commit.parents.iter().enumerate() {
                    let to = place(parent, &mut next_tracks, &mut next_lane, || {
                        if parent_index == 0 {
                            commit_color
                        } else {
                            allocate_color(&mut palette_cursor)
                        }
                    });
                    let color = if parent_index == 0 {
                        commit_color
                    } else {
                        next_tracks[to].color
                    };
                    segments.push(Segment {
                        from: Lane(lane_index as u16),
                        to: Lane(to as u16),
                        color,
                    });
                }
            } else {
                let to = place(track.commit, &mut next_tracks, &mut next_lane, || {
                    track.color
                });
                segments.push(Segment {
                    from: Lane(index as u16),
                    to: Lane(to as u16),
                    color: track.color,
                });
            }
        }

        width = width.max(tracks.len() as u16).max(next_tracks.len() as u16);

        rows.push(GraphRow {
            commit: commit.id,
            lane: Lane(lane_index as u16),
            color: commit_color,
            segments,
            has_incoming: was_reserved,
        });

        tracks = next_tracks;
        track_lane = next_lane;
    }

    GraphLayout { rows, width }
}

/// Finds `commit`'s column in `tracks`, opening a new one at the end with `color_if_new()`
/// if none expects it yet.
///
/// The returned index is always in bounds for `tracks`: either it names an entry already
/// there, or `commit` was just pushed onto the end at that same index.
fn place(
    commit: ObjectId,
    tracks: &mut Vec<Track>,
    lane: &mut HashMap<ObjectId, usize>,
    color_if_new: impl FnOnce() -> LaneColor,
) -> usize {
    *lane.entry(commit).or_insert_with(|| {
        let index = tracks.len();
        tracks.push(Track {
            commit,
            color: color_if_new(),
        });
        index
    })
}

/// Hands out the next palette slot, cycling through [`PALETTE_SIZE`] colours.
///
/// Called once per newly opened track, never on a track that merely continues, so that
/// colours stay a proxy for track identity rather than for lane index.
fn allocate_color(cursor: &mut u8) -> LaneColor {
    let color = LaneColor(*cursor % PALETTE_SIZE);
    *cursor = cursor.wrapping_add(1);
    color
}
