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

use domain::{CommitSummary, ObjectId};

use crate::model::{GraphLayout, GraphRow, IncomingLink, Lane, LaneColor, PALETTE_SIZE, Segment};

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
    let mut tracks: Vec<Option<Track>> = Vec::new();
    let mut rows: Vec<GraphRow> = Vec::with_capacity(commits.len());
    let mut width: u16 = 0;
    let mut palette_cursor: u8 = 0;

    for commit in commits {
        let expecting = column_expecting(&tracks, commit.id);
        let was_reserved = expecting.is_some();
        let (lane_index, commit_color) = match expecting {
            Some(found) => found,
            None => {
                let color = allocate_color(&mut palette_cursor);
                let index = free_column(&mut tracks);
                tracks[index] = Some(Track {
                    commit: commit.id,
                    color,
                });
                (index, color)
            }
        };

        let mut next = tracks.clone();
        next[lane_index] = None;

        let mut parent_links: Vec<Segment> = Vec::with_capacity(commit.parents.len());
        let mut convergences: Vec<Segment> = Vec::new();

        for (parent_index, parent) in commit.parents.iter().enumerate() {
            let first = parent_index == 0;
            let waiting = columns_expecting(&next, parent);
            let target = match (first, waiting.first()) {
                (true, Some(&leftmost)) => leftmost.min(lane_index),
                (true, None) => lane_index,
                (false, Some(&leftmost)) => leftmost,
                (false, None) => free_column(&mut next),
            };

            for &column in waiting.iter().filter(|&&column| column != target) {
                if let Some(track) = next[column] {
                    convergences.push(Segment {
                        from: Lane(column as u16),
                        to: Lane(target as u16),
                        color: track.color,
                    });
                }
                next[column] = None;
            }

            let track_color = match next[target] {
                Some(track) => track.color,
                None if first => commit_color,
                None => allocate_color(&mut palette_cursor),
            };
            next[target] = Some(Track {
                commit: parent,
                color: track_color,
            });
            parent_links.push(Segment {
                from: Lane(lane_index as u16),
                to: Lane(target as u16),
                color: if first { commit_color } else { track_color },
            });
        }

        let mut segments: Vec<Segment> = Vec::with_capacity(tracks.len());
        for (index, slot) in tracks.iter().enumerate() {
            if index == lane_index {
                segments.extend(parent_links.iter().copied());
                continue;
            }
            let Some(track) = slot else { continue };
            match convergences
                .iter()
                .find(|segment| segment.from.0 as usize == index)
            {
                Some(converged) => segments.push(*converged),
                None => segments.push(Segment {
                    from: Lane(index as u16),
                    to: Lane(index as u16),
                    color: track.color,
                }),
            }
        }

        trim_trailing_free(&mut next);
        width = width.max(tracks.len() as u16).max(next.len() as u16);

        let lane = Lane(lane_index as u16);
        let incoming = if was_reserved {
            rows.last().map_or_else(Vec::new, |previous: &GraphRow| {
                previous
                    .segments
                    .iter()
                    .filter(|segment| segment.to == lane)
                    .map(|segment| IncomingLink {
                        from: segment.from,
                        color: segment.color,
                    })
                    .collect()
            })
        } else {
            Vec::new()
        };

        if let Some(previous) = rows.last_mut() {
            previous.next_lane = Some(lane);
        }

        rows.push(GraphRow {
            commit: commit.id,
            lane,
            color: commit_color,
            segments,
            incoming,
            next_lane: None,
        });

        tracks = next;
    }

    GraphLayout { rows, width }
}

/// Every column waiting for `commit`, leftmost first.
///
/// More than one can be waiting: two tracks reaching the same ancestor both hold it until
/// the row that places it, and that row is where they converge.
fn columns_expecting(tracks: &[Option<Track>], commit: ObjectId) -> Vec<usize> {
    tracks
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| slot.filter(|track| track.commit == commit).map(|_| index))
        .collect()
}

/// The column already waiting for `commit`, with the colour it has carried.
fn column_expecting(tracks: &[Option<Track>], commit: ObjectId) -> Option<(usize, LaneColor)> {
    tracks.iter().enumerate().find_map(|(index, slot)| {
        slot.filter(|track| track.commit == commit)
            .map(|track| (index, track.color))
    })
}

/// The leftmost column no track is using, opening one on the right if all are taken.
///
/// A column is only ever handed to a track that is starting. One already running keeps
/// its column for its whole life, which is the point: a branch that shifts sideways every
/// time an unrelated track ends is one the eye cannot follow.
fn free_column(tracks: &mut Vec<Option<Track>>) -> usize {
    match tracks.iter().position(Option::is_none) {
        Some(index) => index,
        None => {
            tracks.push(None);
            tracks.len() - 1
        }
    }
}

/// Drops columns freed at the right-hand end, so the gutter is only as wide as the
/// tracks actually reach.
fn trim_trailing_free(tracks: &mut Vec<Option<Track>>) {
    while tracks.last().is_some_and(Option::is_none) {
        tracks.pop();
    }
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
