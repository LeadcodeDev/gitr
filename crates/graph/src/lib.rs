//! Commit graph lane layout.
//!
//! Pure: no I/O, no gpui, no clock. A layout is a function of the commit sequence it is
//! given, which is what makes it testable against hand-written histories.
//!
//! The input sequence must already be in topological order with date priority. Order is
//! not this crate's job and it is not a detail — measured on `rust-lang/cargo`, 23 789
//! commits, the same placement algorithm yields 258 lanes on a date-ordered walk and 20
//! on a topologically ordered one.

pub mod layout;
pub mod model;

pub use layout::layout;
pub use model::{GraphLayout, GraphRow, IncomingLink, Lane, LaneColor, PALETTE_SIZE, Segment};
