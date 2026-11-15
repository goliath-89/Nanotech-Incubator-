use super::{EntryInfo, RootCtx, Volume, VolumeKind, VolumeUsage};
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

// ---------------------------------------------------------------------------
// Listing volumes
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub(super) fn volumes() -> io::Result<Vec<Volume>> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStringExt;

    // Ask how many mounts there are, then fetch them into our own buffer
    // (getmntinfo would hand back a shared static one).
    // SAFETY: a null buffer with size 0 only returns the count.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    // Room for a few mounts that appear between the two calls.
    let capacity = count as usize + 8;
    // SAFETY: statfs is plain data; all-zero is a valid value for it.
    let mut mounts: Vec<libc::statfs> = vec![unsafe { std::mem::zeroed() }; capacity];
    let bytes = (capacity * std::mem::size_of::<libc::statfs>()) as libc::c_int;
    // SAFETY: the buffer is `bytes` long and writable; the call fills at
    // most that much and returns how many entries it wrote.
    let filled = unsafe { libc::getfsstat(mounts.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    if filled < 0 {
        return Err(io::Error::last_os_error());
    }
    mounts.truncate(filled as usize);

    let text = |chars: &[libc::c_char]| -> Vec<u8> {
        // SAFETY: the kernel NUL-terminates these fixed-size name fields;
        // the slice bounds the read in any case.
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(chars.as_ptr().cast(), chars.len()) };
        CStr::from_bytes_until_nul(bytes)
            .map(|c| c.to_bytes().to_vec())
            .unwrap_or_default()
    };

    let mut list = Vec::new();
    for m in &mounts {
        let mount_bytes = text(&m.f_mntonname);
        let mount = std::path::PathBuf::from(std::ffi::OsString::from_vec(mount_bytes.clone()));
        let file_system = String::from_utf8_lossy(&text(&m.f_fstypename)).to_lowercase();
        let device = String::from_utf8_lossy(&text(&m.f_mntfromname)).into_owned();
        let flags = m.f_flags as libc::c_int;
        let is_root = mount_bytes == b"/";
        // "Don't browse" marks the system's own helper volumes (VM, Preboot,
        // Update, the Data volume that Finder shows as part of the root).
        if (flags & libc::MNT_DONTBROWSE != 0 && !is_root)
            || matches!(file_system.as_str(), "devfs" | "autofs")
        {
            continue;
        }
        let unit = m.f_bsize as u64;
        let usage = VolumeUsage {
            total_bytes: m.f_blocks * unit,
            free_bytes: m.f_bfree * unit,
            available_bytes: m.f_bavail * unit,
        };
        if usage.total_bytes == 0 {
            continue;
        }
        let kind = if flags & libc::MNT_LOCAL == 0 {
            VolumeKind::Network
        } else if is_root {
            VolumeKind::Internal
        } else {
            VolumeKind::External
        };
        let (name, uuid) = mac_name_and_uuid(&mount_bytes);
        let fallback_name = mount
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Macintosh HD".to_string());
        list.push(Volume {
            id: uuid.unwrap_or(device),
            name: name.filter(|n| !n.is_empty()).unwrap_or(fallback_name),
            mount,
            file_system,
            kind,
            read_only: flags & libc::MNT_RDONLY != 0,
            usage,
        });
    }
    Ok(list)
}

/// The volume's display name and UUID, where the file system has them.
#[cfg(target_os = "macos")]
fn mac_name_and_uuid(mount: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(path) = CString::new(mount) else {
        return (None, None);
    };
    let mut request = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_NAME | libc::ATTR_VOL_UUID,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // Reply layout: u32 total length; then, in bit order, the name as an
    // (offset, length) reference and the 16-byte UUID; then the name's text.
    // A u32-aligned buffer keeps the header fields aligned.
    let mut reply = [0u32; 320];
    let reply_bytes = std::mem::size_of_val(&reply);
    // SAFETY: `path` is NUL-terminated, `request` is a valid attrlist, and
    // the reply buffer is writable for `reply_bytes` bytes.
    let rc = unsafe {
        libc::getattrlist(
            path.as_ptr(),
            (&mut request as *mut libc::attrlist).cast(),
            reply.as_mut_ptr().cast(),
            reply_bytes,
            0,
        )
    };
    if rc != 0 {
        return (None, None);
    }
    // SAFETY: reading the buffer we own as bytes, within its size.
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(reply.as_ptr().cast(), reply_bytes) };
    let u32_at =
        |at: usize| u32::from_ne_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let written = (u32_at(0) as usize).min(reply_bytes);
    if written < 4 + 8 + 16 {
        return (None, None);
    }
    // The name's offset counts from the reference itself, at byte 4.
    let offset = u32_at(4) as i32;
    let length = u32_at(8) as usize; // includes the terminating NUL
    let name = usize::try_from(offset)
        .ok()
        .map(|o| 4 + o)
        .and_then(|start| {
            let end = start.checked_add(length.checked_sub(1)?)?;
            (end <= written).then(|| String::from_utf8_lossy(&bytes[start..end]).into_owned())
        });
    let u = &bytes[12..28];
    let hex = |range: std::ops::Range<usize>| {
        u[range]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>()
    };
    let uuid = (!u.iter().all(|b| *b == 0)).then(|| {
        format!(
            "{}-{}-{}-{}-{}",
            hex(0..4),
            hex(4..6),
            hex(6..8),
            hex(8..10),
            hex(10..16)
        )
    });
    (name, uuid)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn volumes() -> io::Result<Vec<Volume>> {
    /// File system types that are not storage a person would scan.
    const PSEUDO: &[&str] = &[
        "proc",
        "sysfs",
        "tmpfs",
        "devtmpfs",
        "devpts",
        "cgroup",
        "cgroup2",
        "pstore",
        "bpf",
        "debugfs",
        "tracefs",
        "securityfs",
        "configfs",
        "fusectl",
        "mqueue",
        "hugetlbfs",
        "autofs",
        "binfmt_misc",
        "efivarfs",
        "ramfs",
        "squashfs",
        "nsfs",
        "rpc_pipefs",
        "selinuxfs",
        "fuse.portal",
    ];
    const NETWORK: &[&str] = &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "sshfs",
        "fuse.sshfs",
        "9p",
        "afs",
        "ceph",
    ];

    let table = fs::read_to_string("/proc/self/mounts")?;
    let mut list: Vec<Volume> = Vec::new();
    for line in table.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(mount), Some(file_system), Some(options)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let mount = unescape_mount(mount);
        let is_root = mount == "/";
        let network = NETWORK.contains(&file_system);
        if PSEUDO.contains(&file_system) || !(is_root || network || device.starts_with("/dev/")) {
            continue;
        }
        let path = std::path::PathBuf::from(&mount);
        let Ok(usage) = volume_usage(&path) else {
            continue; // not reachable by this user
        };
        // The same device mounted in several places is one volume; keep the
        // mount with the shortest path.
        if usage.total_bytes == 0 {
            continue;
        }
        if let Some(existing) = list.iter_mut().find(|v| v.id == device) {
            if mount.len() < existing.mount.as_os_str().len() {
                existing.mount = path;
            }
            continue;
        }
        let removable = mount.starts_with("/media/")
            || mount.starts_with("/run/media/")
            || mount.starts_with("/mnt/");
        list.push(Volume {
            id: device.to_string(),
            name: if is_root {
                "System".to_string()
            } else {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| mount.clone())
            },
            mount: path,
            file_system: file_system.to_lowercase(),
            kind: if network {
                VolumeKind::Network
            } else if removable {
                VolumeKind::External
            } else {
                VolumeKind::Internal
            },
            read_only: options.split(',').any(|o| o == "ro"),
            usage,
        });
    }
    Ok(list)
}

/// `/proc/self/mounts` writes spaces and other odd bytes as `\040`-style octal.
#[cfg(not(target_os = "macos"))]
fn unescape_mount(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            .filter(|d| bytes[i] == b'\\' && d.iter().all(|b| (b'0'..=b'7').contains(b)));
        match octal {
            Some(d) => {
                out.push((d[0] - b'0') * 64 + (d[1] - b'0') * 8 + (d[2] - b'0'));
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
