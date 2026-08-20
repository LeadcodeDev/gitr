//! Pure mapping from a [`GraphRow`] to the coordinates the gutter paints.
//!
//! Kept free of `gpui`'s `App` and `Window`: only [`Point`], [`Pixels`] and [`px`] are
//! needed, none of which require a live window, so the mapping is asserted directly
//! rather than through a rendered frame.

use gpui::{Pixels, Point, point, px};
use graph::{GraphRow, Lane, LaneColor};

/// Horizontal distance between two adjacent lane centres.
pub const LANE_SPACING: Pixels = px(12.);

/// Radius of a commit's own node.
pub const NODE_RADIUS: Pixels = px(3.);

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

/// One [`graph::Segment`], translated into the two points a renderer draws between.
///
/// Coordinates are relative to the row band's own top-left corner; a renderer adds the
/// band's bounds origin to reach window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentGeometry {
    pub top: Point<Pixels>,
    /// Where the line stops running straight down and slants across, if it does.
    ///
    /// A lane change is a short slant near the edge rather than a lean across the whole
    /// band: the line belongs to its lane, and reads as belonging to it, right up to the
    /// point where it leaves.
    pub bend: Option<Point<Pixels>>,
    pub bottom: Point<Pixels>,
    pub color: LaneColor,
}

/// Everything a renderer needs to paint one [`GraphRow`]'s band.
///
/// Split into three because a line must stop at a node rather than run through it. A
/// crossing spans the whole band; an outgoing link starts at the node's centre, half a band
/// lower; the incoming stub covers the half above the node and is absent on a branch tip,
/// which nothing points at from above.
///
/// Drawn as one full-height segment instead, a converging track leaves its lane at the top
/// edge and has already drifted sideways by mid-height, so the node it belongs to sits
/// beside its own line rather than on it. A root commit fares worse: it emits no segment at
/// all, so the line from above ends half a band short of the node it should land on.
#[derive(Clone, Debug, PartialEq)]
pub struct RowGeometry {
    pub node_center: Point<Pixels>,
    pub node_color: LaneColor,
    pub incoming: Vec<SegmentGeometry>,
    pub crossings: Vec<SegmentGeometry>,
    pub outgoing: Vec<SegmentGeometry>,
}

/// Maps `row` onto row-relative coordinates for a band `row_height` tall with lanes
/// `lane_spacing` apart.
pub fn row_geometry(row: &GraphRow, row_height: Pixels, lane_spacing: Pixels) -> RowGeometry {
    let node_center = point(lane_center_x(row.lane, lane_spacing), row_height * 0.5);

    let mut crossings = Vec::with_capacity(row.segments.len());
    let mut outgoing = Vec::new();

    for segment in &row.segments {
        let is_outgoing = row.is_outgoing(segment);
        let from_x = lane_center_x(segment.from, lane_spacing);
        let top = if is_outgoing {
            node_center
        } else {
            point(from_x, Pixels::ZERO)
        };

        let geometry = if segment.is_vertical() {
            SegmentGeometry {
                top,
                bend: None,
                bottom: point(from_x, row_height),
                color: segment.color,
            }
        } else if is_outgoing && row.lands_on_next_node(segment) {
            SegmentGeometry {
                top,
                bend: Some(point(from_x, row_height * 0.5 + NODE_RADIUS)),
                bottom: point(
                    midpoint_x(segment.from, segment.to, lane_spacing),
                    row_height,
                ),
                color: segment.color,
            }
        } else {
            let slant = slant_height(segment.from, segment.to, lane_spacing, row_height - top.y);
            SegmentGeometry {
                top,
                bend: Some(point(from_x, row_height - slant)),
                bottom: point(lane_center_x(segment.to, lane_spacing), row_height),
                color: segment.color,
            }
        };

        if is_outgoing {
            outgoing.push(geometry);
        } else {
            crossings.push(geometry);
        }
    }

    let incoming = row
        .incoming
        .iter()
        .map(|link| {
            if link.from == row.lane {
                return SegmentGeometry {
                    top: point(node_center.x, Pixels::ZERO),
                    bend: None,
                    bottom: node_center,
                    color: link.color,
                };
            }
            SegmentGeometry {
                top: point(midpoint_x(link.from, row.lane, lane_spacing), Pixels::ZERO),
                bend: Some(point(node_center.x, row_height * 0.5 - NODE_RADIUS)),
                bottom: node_center,
                color: link.color,
            }
        })
        .collect();

    RowGeometry {
        node_center,
        node_color: row.color,
        incoming,
        crossings,
        outgoing,
    }
}

/// How much vertical room a lane change is given before it must be complete.
///
/// One lane of sideways travel per lane of downward travel, so a slant sits at forty-five
/// degrees however many lanes it crosses — and is clamped to the room actually available,
/// which is what keeps a wide jump inside its band instead of running past the edge.
///
/// Only crossings use this. A link that touches a node runs to the node's edge instead,
/// so that no straight stub is left between the slant and the circle.
fn slant_height(from: Lane, to: Lane, lane_spacing: Pixels, available: Pixels) -> Pixels {
    let lanes = from.0.abs_diff(to.0).max(1) as usize;
    (lane_spacing * lanes).min(available)
}

/// Where a line between two lane centres crosses the edge between two row bands.
///
/// A link from a node to the node one row below spans half of each band, so the edge cuts
/// it exactly in half. Both bands compute this from their own end and arrive at the same
/// x, which is what makes the two halves one straight line rather than a dogleg.
fn midpoint_x(from: Lane, to: Lane, lane_spacing: Pixels) -> Pixels {
    (lane_center_x(from, lane_spacing) + lane_center_x(to, lane_spacing)) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ObjectId;
    use graph::{IncomingLink, Segment};

    fn id() -> ObjectId {
        "a".repeat(40).parse().unwrap()
    }

    fn row(lane: u16, has_incoming: bool, segments: Vec<Segment>) -> GraphRow {
        GraphRow {
            commit: id(),
            lane: Lane(lane),
            color: LaneColor(0),
            segments,
            incoming: if has_incoming {
                vec![IncomingLink {
                    from: Lane(lane),
                    color: LaneColor(0),
                }]
            } else {
                Vec::new()
            },
            next_lane: None,
        }
    }

    fn landing_on(mut row: GraphRow, next_lane: u16) -> GraphRow {
        row.next_lane = Some(Lane(next_lane));
        row
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
    fn an_outgoing_link_leaves_from_the_node_not_from_the_top_of_the_band() {
        let geometry = row_geometry(&row(1, true, vec![segment(1, 0)]), px(24.), px(16.));

        assert_eq!(geometry.node_center, point(px(24.), px(12.)));
        assert_eq!(geometry.outgoing.len(), 1);
        assert!(geometry.crossings.is_empty());

        let link = geometry.outgoing[0];
        assert_eq!(
            link.top, geometry.node_center,
            "a converging track must leave its own node, or the node sits beside its line"
        );
        assert_eq!(link.bottom, point(px(8.), px(24.)));
    }

    #[test]
    fn a_link_to_the_node_one_row_down_meets_it_halfway_from_both_sides() {
        let above = landing_on(row(1, false, vec![segment(1, 0)]), 0);
        let below = GraphRow {
            commit: id(),
            lane: Lane(0),
            color: LaneColor(0),
            segments: vec![],
            incoming: vec![IncomingLink {
                from: Lane(1),
                color: LaneColor(2),
            }],
            next_lane: None,
        };

        let top_half = row_geometry(&above, px(24.), px(16.));
        let bottom_half = row_geometry(&below, px(24.), px(16.));

        let leaves = top_half.outgoing[0];
        let arrives = bottom_half.incoming[0];

        assert_eq!(
            leaves.bottom.x, arrives.top.x,
            "the two halves of one straight line must meet at the same x, or it doglegs"
        );
        assert_eq!(leaves.top, top_half.node_center);
        assert_eq!(arrives.bottom, bottom_half.node_center);
        assert_eq!(leaves.bottom.x, px(16.));

        let leaves_bend = leaves
            .bend
            .expect("the upper half runs straight down first");
        let arrives_bend = arrives.bend.expect("the lower half straightens up again");
        assert_eq!(
            leaves_bend.x, leaves.top.x,
            "the line must stay in its own lane until it bends"
        );
        assert_eq!(
            arrives_bend.x, arrives.bottom.x,
            "and must be back in the destination lane before reaching the node"
        );
        assert!(
            leaves_bend.y > leaves.top.y,
            "the vertical run comes before the slant, not after"
        );
    }

    #[test]
    fn a_lane_merely_reserved_for_a_later_commit_stays_in_its_own_lane() {
        let geometry = row_geometry(
            &landing_on(row(1, false, vec![segment(1, 0)]), 1),
            px(24.),
            px(16.),
        );

        assert_eq!(
            geometry.outgoing[0].bottom,
            point(px(8.), px(24.)),
            "with the next node elsewhere, the link must reach its lane and meet a vertical"
        );
    }

    #[test]
    fn a_crossing_spans_the_whole_band() {
        let geometry = row_geometry(&row(1, true, vec![segment(0, 0)]), px(24.), px(16.));

        assert_eq!(geometry.crossings.len(), 1);
        assert!(geometry.outgoing.is_empty());

        let crossing = geometry.crossings[0];
        assert_eq!(crossing.bend, None);
        assert_eq!(crossing.top, point(px(8.), px(0.)));
        assert_eq!(crossing.bottom, point(px(8.), px(24.)));
    }

    #[test]
    fn a_commit_with_a_child_above_gets_a_stub_down_into_its_node() {
        let geometry = row_geometry(&row(1, true, vec![]), px(24.), px(16.));

        assert_eq!(geometry.incoming.len(), 1);
        let stub = geometry.incoming[0];
        assert_eq!(stub.bend, None);
        assert_eq!(stub.top, point(px(24.), px(0.)));
        assert_eq!(
            stub.bottom, geometry.node_center,
            "the line from above stops at the node, not half a band short of it"
        );
    }

    #[test]
    fn a_branch_tip_has_nothing_drawn_above_it() {
        let geometry = row_geometry(&row(1, false, vec![segment(1, 1)]), px(24.), px(16.));

        assert!(
            geometry.incoming.is_empty(),
            "nothing points at a tip from above, so no line may be drawn there"
        );
        assert_eq!(geometry.outgoing.len(), 1);
        assert_eq!(geometry.outgoing[0].top, geometry.node_center);
    }

    #[test]
    fn a_root_commit_draws_nothing_below_its_node() {
        let geometry = row_geometry(&row(0, true, vec![]), px(24.), px(16.));

        assert!(geometry.outgoing.is_empty());
        assert!(geometry.crossings.is_empty());
        assert_eq!(geometry.incoming[0].bottom, geometry.node_center);
    }

    #[test]
    fn a_root_that_other_lanes_pass_still_shows_them() {
        let geometry = row_geometry(&row(0, true, vec![segment(1, 1)]), px(24.), px(16.));

        assert_eq!(geometry.crossings.len(), 1);
        assert!(geometry.outgoing.is_empty());
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
    fn the_gutter_is_wider_than_one_lane_as_soon_as_there_are_two() {
        assert!(gutter_width(2, LANE_SPACING) > LANE_SPACING);
        assert_eq!(gutter_width(2, LANE_SPACING), LANE_SPACING * 2usize);
    }
}
