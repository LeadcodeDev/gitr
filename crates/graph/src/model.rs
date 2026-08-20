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
    /// Lines crossing the lower half, from the row's own centre down to its bottom edge.
    ///
    /// `from` is the column at the centre, `to` the column at the edge. They differ only
    /// for a merge's second parent, which leaves the node sideways.
    pub segments: Vec<Segment>,
    /// Lines crossing the upper half, from the top edge down to the row's own centre.
    ///
    /// Empty on a branch tip, which nothing points at from above.
    pub incoming: Vec<Segment>,
}

impl GraphRow {
    /// Whether any line from the row above reaches this commit's own node.
    ///
    /// Not the same as having incoming lines at all: a row carries one for every column
    /// crossing it, and a branch tip is crossed by its neighbours while nothing points at
    /// the tip itself.
    pub fn has_incoming(&self) -> bool {
        self.incoming.iter().any(|segment| segment.to == self.lane)
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
