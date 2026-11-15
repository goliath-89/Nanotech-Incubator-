//! Moving things to the Trash, after every check has passed.

use crate::{Policy, Reason, Refusal};
use std::fs;
use std::path::{Path, PathBuf};

/// Why an item was not moved.
#[derive(Debug)]
pub enum VetError {
    /// The policy says no.
    Refused(Refusal),
    /// The item could not be examined (usually: it is already gone).
    Unreadable(std::io::Error),
}

impl From<Refusal> for VetError {
    fn from(r: Refusal) -> Self {
        VetError::Refused(r)
    }
}

impl From<Reason> for VetError {
    fn from(r: Reason) -> Self {
        VetError::Refused(r.into())
    }
}

#[derive(Debug, Default)]
pub struct TrashReport {
    /// Now in the Trash, as the paths that were asked for.
    pub moved: Vec<PathBuf>,
    /// Not moved because the policy protects them.
    pub refused: Vec<(PathBuf, Refusal)>,
    /// Not moved because something went wrong; the text says what.
    pub failed: Vec<(PathBuf, String)>,
}

/// Run every check on one item without moving it. Returns the item's real
/// location (links in the folders above it resolved).
///
/// The item must pass the policy as written, lie strictly inside
/// `scan_root`, exist, not be a link, pass the policy again at its real
/// location, still lie inside the real scan root, and not be another drive's
/// mount point.
pub fn vet(policy: &Policy, scan_root: &Path, item: &Path) -> Result<PathBuf, VetError> {
    policy.check(item)?;
    strictly_inside(policy, item, scan_root)?;

    let meta = fs::symlink_metadata(item).map_err(VetError::Unreadable)?;
    if meta.file_type().is_symlink() {
        return Err(Reason::Link.into());
    }

    // The text of a path can lie: a folder above the item may be a link into
    // a protected area, or (on Windows) a short alias of a protected name.
    // Resolve the folder that holds the item and check again.
    let (Some(parent), Some(name)) = (item.parent(), item.file_name()) else {
        return Err(Reason::VolumeRoot.into());
    };
    let real_parent = fs::canonicalize(parent).map_err(VetError::Unreadable)?;
    let real = real_parent.join(name);
    policy.check(&real)?;
    let real_root = fs::canonicalize(scan_root).map_err(VetError::Unreadable)?;
    strictly_inside(policy, &real, &real_root)?;

    if meta.is_dir() && is_mount_point(&meta, &real_parent) {
        return Err(Reason::MountPoint.into());
    }
    Ok(real)
}

fn strictly_inside(policy: &Policy, item: &Path, root: &Path) -> Result<(), Refusal> {
    let item = policy.parse(item)?;
    let root = policy.parse(root)?;
    if item.starts_with(&root) && item.parts.len() > root.parts.len() {
        Ok(())
    } else {
        Err(Reason::OutsideScan.into())
    }
}

#[cfg(unix)]
fn is_mount_point(meta: &fs::Metadata, real_parent: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match fs::metadata(real_parent) {
        Ok(parent) => parent.dev() != meta.dev(),
        Err(_) => true, // cannot tell, so assume the worse case
    }
}

/// On Windows a mounted volume or junction is a reparse point, which
/// `symlink_metadata` reports as a link; those were refused already.
#[cfg(not(unix))]
fn is_mount_point(_meta: &fs::Metadata, _real_parent: &Path) -> bool {
    false
}

/// Move `items` to the Trash or Recycle Bin. Items that fail a check are
/// reported and left alone; the rest are moved. Nothing is ever deleted
/// outright.
pub fn trash(policy: &Policy, scan_root: &Path, items: &[PathBuf]) -> TrashReport {
    let mut report = TrashReport::default();
    let mut accepted: Vec<(PathBuf, PathBuf)> = Vec::new(); // (as asked, real)
    for item in items {
        match vet(policy, scan_root, item) {
            Ok(real) => accepted.push((item.clone(), real)),
            Err(VetError::Refused(r)) => report.refused.push((item.clone(), r)),
            Err(VetError::Unreadable(e)) => report.failed.push((item.clone(), e.to_string())),
        }
    }

    // Folders before their contents, so an item inside a folder that is also
    // on the list simply goes with the folder.
    accepted.sort_by_key(|(_, real)| real.components().count());
    let mut done: Vec<PathBuf> = Vec::new();
    for (asked, real) in accepted {
        if done.iter().any(|d| real.starts_with(d)) {
            report.moved.push(asked);
            continue;
        }
        match move_to_trash(&real) {
            Ok(()) => {
                report.moved.push(asked);
                done.push(real);
            }
            Err(e) => report.failed.push((asked, e.to_string())),
        }
    }
    report
}

/// On macOS, ask the file manager directly. The `trash` crate's default
/// there drives Finder by script, which needs an extra permission prompt and
/// does not work without a logged-in desktop.
#[cfg(target_os = "macos")]
fn move_to_trash(path: &Path) -> Result<(), ::trash::Error> {
    use ::trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut context = ::trash::TrashContext::new();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context.delete(path)
}

#[cfg(not(target_os = "macos"))]
fn move_to_trash(path: &Path) -> Result<(), ::trash::Error> {
    ::trash::delete(path)
}
