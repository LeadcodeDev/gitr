//! Git adapters.
//!
//! Structured reads go through `gix`, which exploits the `commit-graph` file and is an
//! order of magnitude faster than libgit2 on the same walk. Patch text and every
//! mutating or network operation go through a `git` subprocess, so the user's own
//! configuration applies exactly as it does in their terminal.
