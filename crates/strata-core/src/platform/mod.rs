//! Everything that differs between operating systems lives behind this module.

use std::fs::DirEntry;
use std::io;
use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// What the scanner needs to know about one directory entry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EntryInfo {
    /// A real folder that may be entered. Symbolic links and junctions are not.
    pub is_dir: bool,
    /// Bytes reserved on disk.
    pub alloc: u64,
    /// File length.
    pub logical: u64,
    /// File system the entry lives on (0 where the platform does not say).
    pub dev: u64,
    /// Identity shared by all hard links to the same file; `None` when the
    /// file has a single link.
    pub link_id: Option<(u64, u64)>,
    /// The file's data lives in the cloud and takes no local space.
    pub cloud_only: bool,
}

/// Facts about the scan root, gathered once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RootCtx {
    pub dev: u64,
    /// Bytes the root folder itself occupies.
    pub alloc: u64,
    /// Allocation unit of the volume; only used where the OS does not report
    /// per-file allocation cheaply.
    #[allow(dead_code)]
    pub cluster: u64,
}

pub(crate) fn root_ctx(root: &Path) -> io::Result<RootCtx> {
    imp::root_ctx(root)
}

pub(crate) fn entry_info(entry: &DirEntry, root: &RootCtx) -> io::Result<EntryInfo> {
    imp::entry_info(entry, root)
}

/// Capacity figures for the volume that holds a path, as the OS reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeUsage {
    pub total_bytes: u64,
    /// Free space including blocks reserved for the system.
    pub free_bytes: u64,
    /// Free space an ordinary user can write to.
    pub available_bytes: u64,
}

impl VolumeUsage {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }
}

/// Ask the OS how full the volume holding `path` is.
pub fn volume_usage(path: &Path) -> io::Result<VolumeUsage> {
    imp::volume_usage(path)
}
