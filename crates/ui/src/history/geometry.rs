//! Pure mapping from a [`GraphRow`] to the coordinates the gutter paints.
//!
//! Kept free of `gpui`'s `App` and `Window`: only [`Point`], [`Pixels`] and [`px`] are
//! needed, none of which require a live window, so the mapping is asserted directly
//! rather than through a rendered frame.

use gpui::{Pixels, Point, point, px};
use graph::{GraphRow, Lane, LaneColor};

/// Horizontal distance between two adjacent lane centres.
pub const LANE_SPACING: Pixels = px(16.);

/// Radius of a commit's own node.
pub const NODE_RADIUS: Pixels = px(4.);

/// Stroke width of a graph line.
pub const LINE_WIDTH: Pixels = px(1.5);

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
    pub bottom: Point<Pixels>,
    pub color: LaneColor,
    pub is_vertical: bool,
}

/// Everything a renderer needs to paint one [`GraphRow`]'s band.
#[derive(Clone, Debug, PartialEq)]
pub struct RowGeometry {
    pub node_center: Point<Pixels>,
    pub node_color: LaneColor,
    pub segments: Vec<SegmentGeometry>,
}

/// Maps `row` onto row-relative coordinates for a band `row_height` tall with lanes
/// `lane_spacing` apart.
pub fn row_geometry(row: &GraphRow, row_height: Pixels, lane_spacing: Pixels) -> RowGeometry {
    let node_center = point(lane_center_x(row.lane, lane_spacing), row_height * 0.5);

    let segments = row
        .segments
        .iter()
        .map(|segment| SegmentGeometry {
            top: point(lane_center_x(segment.from, lane_spacing), Pixels::ZERO),
            bottom: point(lane_center_x(segment.to, lane_spacing), row_height),
            color: segment.color,
            is_vertical: segment.is_vertical(),
        })
        .collect();

    RowGeometry {
        node_center,
        node_color: row.color,
        segments,
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
    fn a_straight_vertical_segment_shares_its_top_and_bottom_x() {
        let row = GraphRow {
            commit: id(),
            lane: Lane(1),
            color: LaneColor(0),
            segments: vec![Segment {
                from: Lane(1),
                to: Lane(1),
                color: LaneColor(2),
            }],
        };

        let geometry = row_geometry(&row, px(24.), px(16.));

        assert_eq!(geometry.node_center, point(px(24.), px(12.)));
        assert_eq!(geometry.node_color, LaneColor(0));
        assert_eq!(geometry.segments.len(), 1);
        let segment = geometry.segments[0];
        assert!(segment.is_vertical);
        assert_eq!(segment.top, point(px(24.), px(0.)));
        assert_eq!(segment.bottom, point(px(24.), px(24.)));
        assert_eq!(segment.color, LaneColor(2));
    }

    #[test]
    fn a_diagonal_segment_crosses_from_one_lane_to_another() {
        let row = GraphRow {
            commit: id(),
            lane: Lane(0),
            color: LaneColor(0),
            segments: vec![Segment {
                from: Lane(0),
                to: Lane(1),
                color: LaneColor(1),
            }],
        };

        let geometry = row_geometry(&row, px(24.), px(16.));

        let segment = geometry.segments[0];
        assert!(!segment.is_vertical);
        assert_eq!(segment.top.x, px(8.));
        assert_eq!(segment.bottom.x, px(24.));
    }

    #[test]
    fn a_row_with_no_segments_still_places_the_node() {
        let row = GraphRow {
            commit: id(),
            lane: Lane(0),
            color: LaneColor(0),
            segments: Vec::new(),
        };

        let geometry = row_geometry(&row, px(20.), px(16.));

        assert!(geometry.segments.is_empty());
        assert_eq!(geometry.node_center, point(px(8.), px(10.)));
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
