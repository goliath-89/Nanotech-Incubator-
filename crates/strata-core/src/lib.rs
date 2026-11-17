//! Strata scanner core.
//!
//! Walks a directory tree in parallel and reports how much space each folder
//! really occupies on disk. The rules that keep the totals honest:
//!
//! * sizes are *allocated* bytes (what the file system reserved), not file length;
//! * a file with several hard links is counted once, in a deterministic place;
//! * symbolic links, junctions and other mount points are never followed;
//! * cloud-only placeholder files count as zero bytes on disk and are reported
//!   separately;
//! * anything that could not be read is recorded, never silently dropped.

mod platform;
pub mod raw;
mod scan;
mod tree;

pub use platform::{volume_usage, volumes, Volume, VolumeKind, VolumeUsage};
pub use scan::{scan, ScanControl, ScanOptions, TopLevelSoFar};
pub use tree::{
    Classify, DirFlags, DirNode, FileNode, Kind, NoClassify, ScanError, ScanErrorKind, ScanStats,
    ScanTree, KIND_COUNT,
};
