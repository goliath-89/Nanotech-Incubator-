use super::{EntryInfo, RootCtx, VolumeUsage};
use std::fs::{self, DirEntry};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{GetLastError, NO_ERROR};
use windows_sys::Win32::Storage::FileSystem::{
    GetCompressedFileSizeW, GetDiskFreeSpaceExW, GetDiskFreeSpaceW, INVALID_FILE_SIZE,
};

const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x0000_0200;
const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x0000_0800;
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// Set on OneDrive and other cloud placeholders whose data is not local.
const CLOUD_ONLY: u32 =
    FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
/// Files whose space on disk cannot be derived from their length.
const NEEDS_EXACT_SIZE: u32 = FILE_ATTRIBUTE_SPARSE_FILE | FILE_ATTRIBUTE_COMPRESSED;

/// NTFS keeps very small files inside the file's own index record, where they
/// take no cluster. The exact limit depends on the record's other contents;
/// this is a conservative lower bound.
const RESIDENT_MAX: u64 = 512;
const DEFAULT_CLUSTER: u64 = 4096;

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub(super) fn root_ctx(root: &Path) -> io::Result<RootCtx> {
    let md = fs::metadata(root)?;
    if !md.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "scan root is not a folder",
        ));
    }
    Ok(RootCtx {
        dev: 0,
        alloc: 0,
        cluster: cluster_size(root).unwrap_or(DEFAULT_CLUSTER),
    })
}

fn cluster_size(path: &Path) -> Option<u64> {
    let w = wide(path);
    let (mut sectors, mut bytes, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY: `w` is NUL-terminated and the four out-pointers are valid u32s.
    let ok =
        unsafe { GetDiskFreeSpaceW(w.as_ptr(), &mut sectors, &mut bytes, &mut free, &mut total) };
    let cluster = sectors as u64 * bytes as u64;
    (ok != 0 && cluster > 0).then_some(cluster)
}

pub(super) fn entry_info(entry: &DirEntry, root: &RootCtx) -> io::Result<EntryInfo> {
    // On Windows this metadata comes with the directory listing: no extra call.
    let md = entry.metadata()?;
    let attrs = md.file_attributes();
    let ft = md.file_type();
    // Symbolic links and junctions report is_dir() == false and are not entered.
    let is_dir = ft.is_dir();
    let logical = if is_dir { 0 } else { md.file_size() };
    let cloud_only = !is_dir && attrs & CLOUD_ONLY != 0;

    let alloc = if is_dir || ft.is_symlink() || cloud_only {
        0
    } else if attrs & NEEDS_EXACT_SIZE != 0 {
        compressed_size(&entry.path()).unwrap_or_else(|| round_up(logical, root.cluster))
    } else if logical <= RESIDENT_MAX {
        0
    } else {
        round_up(logical, root.cluster)
    };

    Ok(EntryInfo {
        is_dir,
        alloc,
        logical,
        dev: 0,
        // Reading the link count needs an open handle per file, which would
        // slow the walk several times over. Hard links are de-duplicated by
        // the MFT scanner instead; see docs/ARCHITECTURE.md.
        link_id: None,
        cloud_only,
    })
}

fn round_up(len: u64, cluster: u64) -> u64 {
    let c = cluster.max(1);
    len.div_ceil(c) * c
}

fn compressed_size(path: &Path) -> Option<u64> {
    let w = wide(path);
    let mut high = 0u32;
    // SAFETY: `w` is NUL-terminated and `high` is a valid out-pointer.
    let low = unsafe { GetCompressedFileSizeW(w.as_ptr(), &mut high) };
    // INVALID_FILE_SIZE is also a legal low half, so the error code decides.
    // SAFETY: GetLastError has no preconditions.
    if low == INVALID_FILE_SIZE && unsafe { GetLastError() } != NO_ERROR {
        return None;
    }
    Some(((high as u64) << 32) | low as u64)
}

pub(super) fn volume_usage(path: &Path) -> io::Result<VolumeUsage> {
    let w = wide(path);
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `w` is NUL-terminated and the three out-pointers are valid u64s.
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut available, &mut total, &mut free) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(VolumeUsage {
        total_bytes: total,
        free_bytes: free,
        available_bytes: available,
    })
}
