//! What Strata offers to clean up.
//!
//! Two engines, both read-only: [`suggest`] names caches, rebuildable
//! developer folders and stale downloads from a finished scan, and
//! [`find_duplicates`] finds files with identical contents. Neither removes
//! anything. Every item they return has already passed the safety policy's
//! text check, and `strata_safety::trash` checks again before moving it.

mod duplicates;
mod suggest;

pub use duplicates::{find_duplicates, DuplicateControl, DuplicateFile, DuplicateGroup};
pub use suggest::{expand, suggest, Group, Options, Suggestion, Target};
