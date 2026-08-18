use domain::ObjectId;

/// A column in the graph gutter. Zero is the leftmost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lane(pub u16);

/// Index into the renderer's palette, not a colour.
///
/// The layout has no business knowing what a theme looks like, and the same layout must
/// render in light and dark mode. The renderer maps this onto theme tokens with
/// `index % PALETTE_SIZE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LaneColor(pub u8);

/// How many distinct colours a renderer must provide.
pub const PALETTE_SIZE: u8 = 8;

/// One line crossing the vertical band of a single row.
///
/// A row occupies a horizontal band. A segment enters that band at lane `from` on the
/// top edge and leaves it at lane `to` on the bottom edge. `from == to` is a straight
/// vertical; otherwise the renderer draws a curve between the two lane centres.
///
/// Segments describe the band as a whole, including lines that merely pass a commit by.
/// A renderer that draws every segment of every visible row, and nothing else, produces
/// the complete graph — it never needs to look at neighbouring rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub from: Lane,
    pub to: Lane,
    pub color: LaneColor,
}

impl Segment {
    pub fn is_vertical(&self) -> bool {
        self.from == self.to
    }
}

/// One commit's placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRow {
    pub commit: ObjectId,
    /// Where the commit's node is drawn.
    pub lane: Lane,
    /// Colour of the node itself, which is the colour of the lane it continues.
    pub color: LaneColor,
    pub segments: Vec<Segment>,
    /// Whether some child above already occupied this commit's lane.
    ///
    /// False for a branch tip, which nothing points at from above. A renderer needs this to
    /// know whether to draw the half-height stub from the top of the band down into the
    /// node: without it, a tip grows a line above it that leads nowhere, and the row below
    /// a commit that has none cannot tell the difference.
    pub has_incoming: bool,
}

impl GraphRow {
    /// Whether `segment` is this commit's own link to a parent rather than an unrelated
    /// lane crossing the band.
    ///
    /// The distinction is what stops a line from being drawn through a node: an outgoing
    /// link starts at the node's centre, half a band below where a crossing line enters.
    pub fn is_outgoing(&self, segment: &Segment) -> bool {
        segment.from == self.lane
    }
}

/// The gutter for one history read.
///
/// `rows` is parallel to the `Vec<CommitSummary>` it was computed from: index `i` here
/// describes commit `i` there. The renderer relies on that, so a layout must never
/// reorder, drop or insert rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphLayout {
    pub rows: Vec<GraphRow>,
    /// One past the highest lane used, so a renderer can size the gutter without scanning.
    pub width: u16,
}

impl GraphLayout {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
