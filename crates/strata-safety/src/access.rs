//! Whether this user may move an item at all, without asking for
//! administrator rights. A read-only check: it opens a handle asking for
//! delete access and closes it again.

use std::path::Path;

/// True when the current user can move `path` (or, with `contents_only`,
/// the items inside the folder `path`) to the Trash. Anything other than a
/// clear "access denied" counts as yes; moving it reports any real problem.
pub fn can_remove(path: &Path, contents_only: bool) -> bool {
    imp::can_remove(path, contents_only)
}

#[cfg(windows)]
mod imp {
    use std::fs::OpenOptions;
    use std::io::ErrorKind;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::Path;

    const DELETE: u32 = 0x0001_0000;
    const FILE_DELETE_CHILD: u32 = 0x0040;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;

    fn allowed(path: &Path, access: u32) -> bool {
        match OpenOptions::new()
            .access_mode(access)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
        {
            Ok(_) => true,
            Err(e) => e.kind() != ErrorKind::PermissionDenied,
        }
    }

    pub fn can_remove(path: &Path, contents_only: bool) -> bool {
        if !contents_only {
            return allowed(path, DELETE);
        }
        if allowed(path, FILE_DELETE_CHILD) {
            return true;
        }
        // The folder may refuse while its items allow it: try the first one.
        match std::fs::read_dir(path) {
            Ok(mut entries) => match entries.next() {
                Some(Ok(e)) => allowed(&e.path(), DELETE),
                _ => true,
            },
            Err(e) => e.kind() != ErrorKind::PermissionDenied,
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    /// Removing an entry needs write access to the folder holding it.
    pub fn can_remove(path: &Path, contents_only: bool) -> bool {
        let folder = if contents_only {
            Some(path)
        } else {
            path.parent()
        };
        match folder.map(std::fs::metadata) {
            Some(Ok(m)) => !m.permissions().readonly(),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_files_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.bin");
        std::fs::write(&f, b"x").unwrap();
        assert!(can_remove(&f, false));
        assert!(can_remove(dir.path(), true));
        assert!(can_remove(&dir.path().join("missing"), false));
    }
}
