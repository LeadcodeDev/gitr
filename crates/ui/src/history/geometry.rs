//! Pure mapping from a [`GraphRow`] to the coordinates the gutter paints.
//!
//! Kept free of `gpui`'s `App` and `Window`: only [`Point`], [`Pixels`] and [`px`] are
//! needed, none of which require a live window, so the mapping is asserted directly
//! rather than through a rendered frame.

use gpui::{Pixels, Point, point, px};
use graph::{GraphRow, Lane, LaneColor};

/// Horizontal distance between two adjacent lane centres.
pub const LANE_SPACING: Pixels = px(12.);

/// Outer radius of a commit's own node, which is the radius of its ring.
///
/// Three quarters of a column, where GitX fills its column edge to edge. Its nodes in
/// adjacent lanes touch; these leave a gap, which is what makes a run of parallel branches
/// read as separate columns rather than as a band.
pub const NODE_RADIUS: Pixels = px(NODE_RADIUS_PX);

/// Radius of the disc filling the node, leaving the ring between the two.
///
/// Subtracted from [`NODE_RADIUS`] rather than taken as a fraction of it, so shrinking the
/// node keeps the ring at its width instead of thinning it away along with everything else.
pub const NODE_INNER_RADIUS: Pixels = px(NODE_RADIUS_PX - NODE_RING_WIDTH_PX);

/// The lengths above, before `px`: `Pixels` keeps its field private, so one constant cannot
/// be derived from another once wrapped.
///
/// The ring is not itself a constant here because nothing paints it — it is what remains
/// between the two discs. GitX's is 1px on a 10px node; this one is a touch wider because
/// the node is smaller, and the ring is the only thing distinguishing a node from the line
/// running through it.
const NODE_RADIUS_PX: f32 = 4.5;
const NODE_RING_WIDTH_PX: f32 = 1.2;

/// Stroke width of a vertical graph line.
pub const LINE_WIDTH: Pixels = px(1.5);

/// Stroke width of a sloped graph line.
///
/// Wider than [`LINE_WIDTH`] on purpose. A vertical stroke lands square on the pixel grid
/// and paints two columns at full strength; the same width on a diagonal spreads across
/// three with two of them faint, so it carries the same ink and reads lighter. Matching
/// the *perceived* weight is what the eye compares, not the declared width.
pub const DIAGONAL_LINE_WIDTH: Pixels = px(2.1);

/// The x coordinate of `lane`'s centre, relative to the gutter's left edge.
pub fn lane_center_x(lane: Lane, lane_spacing: Pixels) -> Pixels {
    lane_spacing * (lane.0 as usize) + lane_spacing * 0.5
}

/// Width of a gutter wide enough for `lane_count` lanes.
///
/// Clamped to at least one lane, so an empty layout still reserves room for the sole
/// commit's own node.
pub fn gutter_width(lane_count: u16, lane_spacing: Pixels) -> Pixels {
    lane_spacing * (lane_count.max(1) as usize)
}

/// One line, as the two points a renderer draws between.
///
/// Always straight. GitX draws every line as a single segment from a cell edge to the
/// cell's own centre, so a change of column happens over half a row and there is no bend
/// anywhere to get wrong.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentGeometry {
    pub top: Point<Pixels>,
    pub bottom: Point<Pixels>,
    pub color: LaneColor,
}

/// Everything a renderer needs to paint one [`GraphRow`]'s cell.
///
/// The cell is split at its own centre, which is where its node sits. Lines above arrive
/// there from the top edge; lines below leave it for the bottom edge. Every one of them
/// ends or starts at that centre, which is what makes a line meet a node rather than pass
/// beside it.
///
/// `node_color` is the one departure from GitX here, which paints every ring black whatever
/// track it belongs to. A ring in the track's own colour says which branch a commit is on
/// without following its line up the gutter, and the hollow centre is what leaves room to
/// say it — a filled disc in the same colour would read as the line, not as a node.
#[derive(Clone, Debug, PartialEq)]
pub struct RowGeometry {
    pub node_center: Point<Pixels>,
    pub node_color: LaneColor,
    pub incoming: Vec<SegmentGeometry>,
    pub outgoing: Vec<SegmentGeometry>,
}

/// Maps `row` onto row-relative coordinates for a cell `row_height` tall with columns
/// `lane_spacing` apart.
pub fn row_geometry(row: &GraphRow, row_height: Pixels, lane_spacing: Pixels) -> RowGeometry {
    let middle = row_height * 0.5;
    let node_center = point(lane_center_x(row.lane, lane_spacing), middle);

    let incoming = row
        .incoming
        .iter()
        .map(|segment| SegmentGeometry {
            top: point(lane_center_x(segment.from, lane_spacing), Pixels::ZERO),
            bottom: point(lane_center_x(segment.to, lane_spacing), middle),
            color: segment.color,
        })
        .collect();

    let outgoing = row
        .segments
        .iter()
        .map(|segment| SegmentGeometry {
            top: point(lane_center_x(segment.from, lane_spacing), middle),
            bottom: point(lane_center_x(segment.to, lane_spacing), row_height),
            color: segment.color,
        })
        .collect();

    RowGeometry {
        node_center,
        node_color: row.color,
        incoming,
        outgoing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ObjectId;
    use graph::Segment;

    fn id() -> ObjectId {
        "a".repeat(40).parse().unwrap()
    }

    fn row(lane: u16, incoming: Vec<Segment>, segments: Vec<Segment>) -> GraphRow {
        GraphRow {
            commit: id(),
            lane: Lane(lane),
            color: LaneColor(0),
            segments,
            incoming,
        }
    }

    fn segment(from: u16, to: u16) -> Segment {
        Segment {
            from: Lane(from),
            to: Lane(to),
            color: LaneColor(2),
        }
    }

    #[test]
    fn lane_center_x_sits_in_the_middle_of_the_lane() {
        assert_eq!(lane_center_x(Lane(0), px(16.)), px(8.));
        assert_eq!(lane_center_x(Lane(1), px(16.)), px(24.));
        assert_eq!(lane_center_x(Lane(3), px(10.)), px(35.));
    }

    #[test]
    fn gutter_width_reserves_at_least_one_lane() {
        assert_eq!(gutter_width(0, px(16.)), px(16.));
        assert_eq!(gutter_width(1, px(16.)), px(16.));
        assert_eq!(gutter_width(3, px(16.)), px(48.));
    }

    #[test]
    fn a_line_from_above_runs_from_the_top_edge_to_the_row_centre() {
        let geometry = row_geometry(&row(0, vec![segment(1, 0)], vec![]), px(24.), px(16.));

        assert_eq!(geometry.incoming.len(), 1);
        let arriving = geometry.incoming[0];
        assert_eq!(arriving.top, point(px(24.), px(0.)));
        assert_eq!(
            arriving.bottom, geometry.node_center,
            "a line changing column arrives at the centre already in the new one, which is \
             what makes it meet the node instead of passing beside it"
        );
    }

    #[test]
    fn a_line_below_runs_from_the_row_centre_to_the_bottom_edge() {
        let geometry = row_geometry(&row(0, vec![], vec![segment(0, 0)]), px(24.), px(16.));

        assert_eq!(geometry.outgoing.len(), 1);
        let leaving = geometry.outgoing[0];
        assert_eq!(leaving.top, point(px(8.), px(12.)));
        assert_eq!(leaving.bottom, point(px(8.), px(24.)));
    }

    #[test]
    fn a_second_parent_leaves_the_node_sideways() {
        let geometry = row_geometry(&row(0, vec![], vec![segment(0, 1)]), px(24.), px(16.));

        let leaving = geometry.outgoing[0];
        assert_eq!(
            leaving.top, geometry.node_center,
            "a merge's second parent starts at the node, not at the column it lands in"
        );
        assert_eq!(leaving.bottom, point(px(24.), px(24.)));
    }

    #[test]
    fn a_branch_tip_has_nothing_drawn_above_it() {
        let geometry = row_geometry(&row(1, vec![], vec![segment(1, 1)]), px(24.), px(16.));

        assert!(
            geometry.incoming.is_empty(),
            "nothing points at a tip from above, so no line may be drawn there"
        );
    }

    #[test]
    fn a_root_commit_draws_nothing_below_its_node() {
        let geometry = row_geometry(&row(0, vec![segment(0, 0)], vec![]), px(24.), px(16.));

        assert!(geometry.outgoing.is_empty());
        assert_eq!(geometry.incoming[0].bottom, geometry.node_center);
    }
}

#[cfg(test)]
mod gutter_fit {
    use super::*;

    #[test]
    fn every_lane_a_layout_declares_fits_inside_the_gutter_it_sizes() {
        for lane_count in 1u16..=32 {
            let width = gutter_width(lane_count, LANE_SPACING);
            for lane in 0..lane_count {
                let center = lane_center_x(Lane(lane), LANE_SPACING);
                assert!(
                    center + NODE_RADIUS <= width,
                    "lane {lane} of {lane_count} centres at {center:?}, past a {width:?} gutter"
                );
                assert!(center - NODE_RADIUS >= Pixels::ZERO);
            }
        }
    }

    #[test]
    fn a_node_stays_hollow_and_clear_of_its_neighbours() {
        assert!(
            NODE_INNER_RADIUS > Pixels::ZERO,
            "shrinking the node past the ring's own width fills it in, and a filled disc in \
             the track's colour reads as the line rather than as a node"
        );
        assert!(
            NODE_RADIUS * 2. < LANE_SPACING,
            "nodes in adjacent columns must not touch, or parallel branches read as a band"
        );
    }

    #[test]
    fn the_gutter_is_wider_than_one_lane_as_soon_as_there_are_two() {
        assert!(gutter_width(2, LANE_SPACING) > LANE_SPACING);
        assert_eq!(gutter_width(2, LANE_SPACING), LANE_SPACING * 2usize);
    }
}
