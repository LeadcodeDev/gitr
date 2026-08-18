//! Views built on gpui-component.
//!
//! Ports are synchronous and blocking, so every call into `vcs` from here runs on
//! `cx.background_executor()`. Nothing in this crate blocks the frame thread, and no
//! long operation opens a modal — that pairing is what makes GitX crash on
//! `assert(currentModalSheet == nil)` when two network operations overlap.

pub mod workspace;

pub use workspace::Workspace;
