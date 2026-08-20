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

use crate::model::{GraphLayout, GraphRow, IncomingLink, Lane, LaneColor, PALETTE_SIZE, Segment};

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
    /// Outgoing columns this commit opened for a second or later parent.
    spawned: Vec<usize>,
    reserved: bool,
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

        let reserved = position.is_some();
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
            next.push(Some(Track {
                commit: parent,
                color: allocate_color(&mut palette_cursor),
            }));
            spawned.push(next.len() - 1);
        }

        width = width.max(lanes.len() as u16).max(next.len() as u16);
        placements.push(Placement {
            position,
            color,
            mapping,
            spawned,
            reserved,
        });
        lanes = next;
    }

    let trailing = identity_mapping(&lanes);
    let rows = assemble(commits, &placements, trailing);
    GraphLayout { rows, width }
}

/// Turns per-row placements into rows, whose segments describe the band *below* them.
///
/// A row's band is bounded by its own outgoing columns above and the next row's below, so
/// the band's segments are the next row's mapping — read one row later than they are
/// computed. A line the row opened for a second parent starts at the node rather than at
/// the column it lands in, which is what `spawned` records.
fn assemble(
    commits: &[CommitSummary],
    placements: &[Placement],
    trailing: Vec<Segment>,
) -> Vec<GraphRow> {
    let mut rows = Vec::with_capacity(commits.len());

    for (index, (commit, placement)) in commits.iter().zip(placements).enumerate() {
        let lane = Lane(placement.position as u16);
        let below = placements.get(index + 1);

        let mut segments = below.map_or_else(|| trailing.clone(), |next| next.mapping.clone());
        for segment in &mut segments {
            if placement.spawned.contains(&(segment.from.0 as usize)) {
                segment.from = lane;
            }
        }

        let incoming = if placement.reserved {
            placement
                .mapping
                .iter()
                .filter(|segment| segment.to == lane)
                .map(|segment| IncomingLink {
                    from: segment.from,
                    color: segment.color,
                })
                .collect()
        } else {
            Vec::new()
        };

        rows.push(GraphRow {
            commit: commit.id,
            lane,
            color: placement.color,
            segments,
            incoming,
            next_lane: below.map(|next| Lane(next.position as u16)),
        });
    }
    rows
}

/// Every surviving column continuing straight down, for the band below the last row.
fn identity_mapping(lanes: &[Option<Track>]) -> Vec<Segment> {
    lanes
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            slot.map(|track: Track| Segment {
                from: Lane(index as u16),
                to: Lane(index as u16),
                color: track.color,
            })
        })
        .collect()
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
