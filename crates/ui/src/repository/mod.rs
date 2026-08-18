//! The seam between the Git adapters and the views.
//!
//! Views render what they are given; they never read a repository themselves. [`model`]
//! holds the shapes handed to them, computed once per load rather than per frame, and the
//! state that owns the reads pushes new ones as they arrive.

pub mod model;
pub mod state;

pub use model::{CommitDetail, History, HistoryFilter, LoadState, ReferenceIndex, RepositoryEvent};
pub use state::RepositoryState;
