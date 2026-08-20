//! Track placement, ported from GitX's `PBGitGrapher.decorateCommit`.
//!
//! A lane is a column holding the [`ObjectId`] it expects next. Each row rebuilds the
//! column list by walking the previous one and appending whatever survives, so a column is
//! a position among survivors and everything right of an ending track slides left. A
//! commit's own node is placed at its index in the *outgoing* list, not the incoming one —
//! that distinction is the whole reason this is a port rather than an adaptation, and it
//! is what a comparison against GitX's output turns on.
//!
//! Convergence is deferred. A commit's first parent takes over its lane without checking
//! whether another column already expects that parent; two columns then hold the same
//! object until the row that places it, and that row is where they meet. Converging
//! eagerly instead closes a column one row early and shifts everything to its right.
//!
//! A commit with no parents leaves a hole rather than closing its column, so the columns
//! beside it keep their positions for one more row.

use domain::{CommitSummary, ObjectId};

use crate::model::{GraphLayout, GraphRow, Lane, LaneColor, PALETTE_SIZE, Segment};

#[derive(Clone, Copy)]
struct Track {
    commit: ObjectId,
    color: LaneColor,
}

/// One row's placement, before the rows below it are known.
struct Placement {
    /// Where this commit's node sits, as an index into the outgoing column list.
    position: usize,
    color: LaneColor,
    /// Incoming column to outgoing column, one entry per line crossing the row.
    mapping: Vec<Segment>,
    /// Columns this commit opened for a second or later parent, with their colours.
    spawned: Vec<(usize, LaneColor)>,
    has_parents: bool,
}

pub fn layout(commits: &[CommitSummary]) -> GraphLayout {
    let mut lanes: Vec<Option<Track>> = Vec::new();
    let mut placements: Vec<Placement> = Vec::with_capacity(commits.len());
    let mut width: u16 = 0;
    let mut palette_cursor: u8 = 0;

    for commit in commits {
        let mut next: Vec<Option<Track>> = Vec::with_capacity(lanes.len() + 1);
        let mut mapping: Vec<Segment> = Vec::with_capacity(lanes.len() + 1);
        let mut position: Option<usize> = None;
        let mut color: Option<LaneColor> = None;

        for (index, slot) in lanes.iter().enumerate() {
            let Some(track) = slot else { continue };
            let to = if track.commit == commit.id {
                match position {
                    None => {
                        next.push(Some(*track));
                        let at = next.len() - 1;
                        position = Some(at);
                        color = Some(track.color);
                        at
                    }
                    Some(at) => at,
                }
            } else {
                next.push(Some(*track));
                next.len() - 1
            };
            mapping.push(Segment {
                from: Lane(index as u16),
                to: Lane(to as u16),
                color: track.color,
            });
        }

        let (position, color) = match (position, color) {
            (Some(at), Some(color)) => (at, color),
            _ => {
                let color = allocate_color(&mut palette_cursor);
                next.push(Some(Track {
                    commit: commit.id,
                    color,
                }));
                (next.len() - 1, color)
            }
        };

        let mut parents = commit.parents.iter();
        next[position] = parents.next().map(|parent| Track {
            commit: parent,
            color,
        });

        let mut spawned = Vec::new();
        for parent in parents {
            if next
                .iter()
                .any(|slot| slot.is_some_and(|track: Track| track.commit == parent))
            {
                continue;
            }
            let color = allocate_color(&mut palette_cursor);
            next.push(Some(Track {
                commit: parent,
                color,
            }));
            spawned.push((next.len() - 1, color));
        }

        width = width.max(lanes.len() as u16).max(next.len() as u16);
        placements.push(Placement {
            position,
            color,
            mapping,
            spawned,
            has_parents: !commit.parents.is_empty(),
        });
        lanes = next;
    }

    let rows = assemble(commits, &placements);
    GraphLayout { rows, width }
}

/// Turns per-row placements into rows, split the way GitX draws a cell: one set of lines
/// crossing the upper half into the row's centre, one crossing the lower half out of it.
///
/// A line that changes column does so in the upper half, arriving at the centre already in
/// its new one. The lower half is therefore vertical, except for a merge's second parent,
/// which leaves the node sideways.
fn assemble(commits: &[CommitSummary], placements: &[Placement]) -> Vec<GraphRow> {
    let mut rows = Vec::with_capacity(commits.len());

    for (commit, placement) in commits.iter().zip(placements) {
        let lane = Lane(placement.position as u16);

        let mut segments: Vec<Segment> = Vec::with_capacity(placement.mapping.len());
        for segment in &placement.mapping {
            if segments.iter().any(|kept: &Segment| kept.to == segment.to) {
                continue;
            }
            segments.push(Segment {
                from: segment.to,
                to: segment.to,
                color: segment.color,
            });
        }
        if placement.has_parents && !segments.iter().any(|segment| segment.to == lane) {
            segments.push(Segment {
                from: lane,
                to: lane,
                color: placement.color,
            });
        }
        segments.retain(|segment| placement.has_parents || segment.to != lane);
        segments.extend(placement.spawned.iter().map(|&(column, color)| Segment {
            from: lane,
            to: Lane(column as u16),
            color,
        }));

        rows.push(GraphRow {
            commit: commit.id,
            lane,
            color: placement.color,
            segments,
            incoming: placement.mapping.clone(),
        });
    }
    rows
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
