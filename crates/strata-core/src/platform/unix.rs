use super::{EntryInfo, RootCtx, VolumeUsage};
use std::ffi::CString;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// `st_blocks` is always in 512-byte units, whatever the file system's block size.
const BLOCK: u64 = 512;

pub(super) fn root_ctx(root: &Path) -> io::Result<RootCtx> {
    // Follow a symbolic link given as the root: the user asked for its target.
    let md = fs::metadata(root)?;
    if !md.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "scan root is not a folder",
        ));
    }
    Ok(RootCtx {
        dev: md.dev(),
        alloc: md.blocks() * BLOCK,
        cluster: md.blksize().max(BLOCK),
    })
}

pub(super) fn entry_info(entry: &DirEntry, _root: &RootCtx) -> io::Result<EntryInfo> {
    // DirEntry::metadata does not follow symbolic links.
    let md = entry.metadata()?;
    let is_dir = md.file_type().is_dir();
    let link_id = if !is_dir && md.nlink() > 1 {
        Some((md.dev(), md.ino()))
    } else {
        None
    };
    Ok(EntryInfo {
        is_dir,
        alloc: md.blocks() * BLOCK,
        logical: if is_dir { 0 } else { md.len() },
        dev: md.dev(),
        link_id,
        cloud_only: is_dataless(&md),
    })
}

/// macOS marks iCloud files whose data is not on this Mac with SF_DATALESS.
#[cfg(target_os = "macos")]
fn is_dataless(md: &Metadata) -> bool {
    use std::os::macos::fs::MetadataExt as _;
    const SF_DATALESS: u32 = 0x4000_0000;
    md.st_flags() & SF_DATALESS != 0
}

#[cfg(not(target_os = "macos"))]
fn is_dataless(_md: &Metadata) -> bool {
    false
}

pub(super) fn volume_usage(path: &Path) -> io::Result<VolumeUsage> {
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))?;
    // SAFETY: `c` is a valid NUL-terminated string and `s` is a properly sized,
    // writable statvfs struct that the call fills in.
    let s = unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return Err(io::Error::last_os_error());
        }
        s
    };
    // Field widths differ between platforms, hence the casts.
    let unit = s.f_frsize as u64;
    Ok(VolumeUsage {
        total_bytes: s.f_blocks as u64 * unit,
        free_bytes: s.f_bfree as u64 * unit,
        available_bytes: s.f_bavail as u64 * unit,
    })
}
