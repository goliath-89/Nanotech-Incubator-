//! A second look at the largest files whose names did not say what they
//! are: read the start of each and, where the contents are recognised,
//! move its space to the right kind in its folder and every folder above.

use std::io::Read;
use strata_classify::{sniff, SNIFF_BYTES};
use strata_core::{Kind, ScanTree};

/// Files at least this large are looked at.
pub const MIN_BYTES: u64 = 50 * 1024 * 1024;
/// At most this many files are read (largest first).
pub const MAX_FILES: usize = 200;

/// Returns how many files changed kind.
pub fn refine_unknown(tree: &mut ScanTree, min_bytes: u64, max_files: usize) -> usize {
    let candidates: Vec<usize> = (0..tree.files.len())
        .filter(|&i| tree.files[i].kind == Kind::Other && tree.files[i].alloc_bytes >= min_bytes)
        .take(max_files)
        .collect();
    let mut changed = 0;
    for i in candidates {
        let f = &tree.files[i];
        let path = tree.dir_path(f.dir).join(&f.name);
        let Some(kind) = read_head(&path).and_then(|h| sniff(&h)) else {
            continue;
        };
        if kind == Kind::Other {
            continue;
        }
        let (dir, bytes) = (f.dir, f.alloc_bytes);
        tree.files[i].kind = kind;
        let mut at = Some(dir);
        while let Some(d) = at {
            let node = &mut tree.dirs[d as usize];
            let other = &mut node.kind_bytes[Kind::Other as usize];
            let moved = bytes.min(*other);
            *other -= moved;
            node.kind_bytes[kind as usize] += moved;
            at = node.parent;
        }
        changed += 1;
    }
    changed
}

/// The first bytes of a regular, local file. Links, cloud-only files and
/// anything unreadable are skipped (reading a cloud-only file downloads it).
fn read_head(path: &std::path::Path) -> Option<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || cloud_only(&meta) {
        return None;
    }
    let mut buf = Vec::with_capacity(SNIFF_BYTES);
    std::fs::File::open(path)
        .ok()?
        .take(SNIFF_BYTES as u64)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

#[cfg(windows)]
fn cloud_only(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // Offline, recall on open, recall on data access.
    meta.file_attributes() & (0x1000 | 0x40000 | 0x400000) != 0
}

#[cfg(target_os = "macos")]
fn cloud_only(meta: &std::fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    meta.st_flags() & 0x4000_0000 != 0 // SF_DATALESS
}

#[cfg(not(any(windows, target_os = "macos")))]
fn cloud_only(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_classify::KindClassifier;
    use strata_core::{scan, ScanControl, ScanOptions};

    #[test]
    fn unknown_large_files_get_their_kind_from_their_contents() {
        // Not under the system's temp folder, whose contents count as caches.
        let tmp = tempfile::Builder::new()
            .prefix("refine-test")
            .tempdir_in(env!("CARGO_MANIFEST_DIR"))
            .unwrap();
        std::fs::create_dir_all(tmp.path().join("a/b")).unwrap();
        let mut movie = b"\0\0\0\x18ftypqt  ".to_vec();
        movie.resize(300_000, 0);
        std::fs::write(tmp.path().join("a/b/capture.raw42"), &movie).unwrap();
        let mut junk = b"nothing to see".to_vec();
        junk.resize(300_000, 1);
        std::fs::write(tmp.path().join("a/blob.bin2"), &junk).unwrap();
        let mut opts = ScanOptions::new(tmp.path());
        opts.min_file_bytes = 1;
        let mut tree = scan(&opts, &KindClassifier, &ScanControl::new()).unwrap();
        let other_before = tree.root().kind_bytes[Kind::Other as usize];
        let videos_before = tree.root().kind_bytes[Kind::Videos as usize];
        let total_before: u64 = tree.root().kind_bytes.iter().sum();

        assert_eq!(refine_unknown(&mut tree, 1, 10), 1);
        let movie_file = tree
            .files
            .iter()
            .find(|f| f.name == "capture.raw42")
            .unwrap();
        assert_eq!(movie_file.kind, Kind::Videos);
        let moved = movie_file.alloc_bytes;
        assert!(moved > 0);
        // Moved from Other to Videos in every folder up to the root.
        assert_eq!(
            tree.root().kind_bytes[Kind::Videos as usize],
            videos_before + moved
        );
        assert_eq!(
            tree.root().kind_bytes[Kind::Other as usize],
            other_before - moved
        );
        let b = &tree.dirs[movie_file.dir as usize];
        assert_eq!(b.kind_bytes[Kind::Videos as usize], moved);
        // Only the split by kind moved; the folder's total is unchanged.
        assert_eq!(tree.root().kind_bytes.iter().sum::<u64>(), total_before);
        assert_eq!(
            tree.files
                .iter()
                .find(|f| f.name == "blob.bin2")
                .unwrap()
                .kind,
            Kind::Other
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_read() {
        let tmp = tempfile::tempdir().unwrap();
        let mut movie = b"\0\0\0\x18ftypqt  ".to_vec();
        movie.resize(300_000, 0);
        std::fs::write(tmp.path().join("real.raw42"), &movie).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("real.raw42"), tmp.path().join("link.raw42"))
            .unwrap();
        assert!(read_head(&tmp.path().join("link.raw42")).is_none());
        assert!(read_head(&tmp.path().join("real.raw42")).is_some());
    }
}
