//! Folders the person chose to leave out of scans.
//!
//! The folder walk skips them as it goes. The NTFS file-table scan reads
//! the whole drive at once, so after any scan [`apply`] makes the result
//! the same as a walk that skipped them: an excluded folder keeps its own
//! entry (flagged `excluded`), loses everything inside it, its sizes come
//! off every folder above it, and nothing inside it is kept or saved.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use strata_core::{DirNode, ScanTree};

/// Same path, as the file system compares them (letter case is ignored on
/// Windows and macOS, whose usual file systems do).
fn same(a: &Path, b: &Path) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        a.as_os_str().to_string_lossy().to_lowercase()
            == b.as_os_str().to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Remove the contents of every folder in `excludes` from `tree`.
/// Excluding the root itself is ignored.
pub fn apply(tree: &mut ScanTree, excludes: &[PathBuf]) {
    if excludes.is_empty() || tree.dirs.len() < 2 {
        return;
    }
    // Find the excluded folders, outermost only (parents come first).
    let mut gone: HashSet<u32> = HashSet::new(); // inside an excluded folder
    let mut hit: Vec<u32> = Vec::new();
    for id in 1..tree.dirs.len() as u32 {
        let parent = tree.dirs[id as usize].parent.unwrap_or(0);
        if gone.contains(&parent) || hit.contains(&parent) {
            gone.insert(id);
            continue;
        }
        let path = tree.dir_path(id);
        if excludes.iter().any(|e| same(e, &path)) {
            hit.push(id);
        }
    }
    if hit.is_empty() {
        return;
    }

    // Empty each excluded folder and take what it held off its ancestors.
    for &e in &hit {
        let d = &tree.dirs[e as usize];
        let below: u64 = d
            .children
            .iter()
            .map(|&c| tree.dirs[c as usize].alloc_bytes)
            .sum();
        let own_block = d.alloc_bytes.saturating_sub(d.own_files_alloc + below);
        let delta = Totals::of(d, own_block);
        let d = &mut tree.dirs[e as usize];
        d.alloc_bytes = own_block;
        d.logical_bytes = d.logical_bytes.saturating_sub(delta.logical);
        d.file_count = 0;
        d.dir_count = 0;
        d.kind_bytes = Default::default();
        d.cloud_only_bytes = 0;
        d.own_files_alloc = 0;
        d.own_file_count = 0;
        d.children.clear();
        d.flags.excluded = true;
        let mut up = d.parent;
        while let Some(p) = up {
            let a = &mut tree.dirs[p as usize];
            delta.take_from(a);
            up = a.parent;
        }
    }

    // Drop everything inside the excluded folders and renumber the rest
    // (parents still come before children).
    let removed_files: HashSet<u32> = gone.iter().copied().chain(hit.iter().copied()).collect();
    let mut new_id = vec![u32::MAX; tree.dirs.len()];
    let mut kept: Vec<DirNode> = Vec::with_capacity(tree.dirs.len() - gone.len());
    for (old, d) in tree.dirs.iter().enumerate() {
        if gone.contains(&(old as u32)) {
            continue;
        }
        new_id[old] = kept.len() as u32;
        kept.push(d.clone());
    }
    for d in &mut kept {
        d.parent = d.parent.map(|p| new_id[p as usize]);
        d.children = d.children.iter().map(|&c| new_id[c as usize]).collect();
    }
    tree.dirs = kept;
    tree.files.retain(|f| !removed_files.contains(&f.dir));
    for f in &mut tree.files {
        f.dir = new_id[f.dir as usize];
    }
}

/// What an excluded folder held, to take off the folders above it.
struct Totals {
    alloc: u64,
    logical: u64,
    files: u64,
    dirs: u64,
    kinds: [u64; strata_core::KIND_COUNT],
    cloud: u64,
}

impl Totals {
    fn of(d: &DirNode, own_block: u64) -> Totals {
        Totals {
            alloc: d.alloc_bytes - own_block,
            // Folders' own entries have no length; all of it is contents.
            logical: d.logical_bytes,
            files: d.file_count,
            dirs: d.dir_count,
            kinds: d.kind_bytes,
            cloud: d.cloud_only_bytes,
        }
    }

    fn take_from(&self, a: &mut DirNode) {
        a.alloc_bytes = a.alloc_bytes.saturating_sub(self.alloc);
        a.logical_bytes = a.logical_bytes.saturating_sub(self.logical);
        a.file_count = a.file_count.saturating_sub(self.files);
        a.dir_count = a.dir_count.saturating_sub(self.dirs);
        for (k, b) in a.kind_bytes.iter_mut().zip(self.kinds) {
            *k = k.saturating_sub(b);
        }
        a.cloud_only_bytes = a.cloud_only_bytes.saturating_sub(self.cloud);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{scan, NoClassify, ScanControl, ScanOptions};

    fn tree_on_disk() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (dir, files) in [
            ("keep", 3),
            ("keep/inner", 2),
            ("secret", 4),
            ("secret/deeper/deepest", 5),
            ("mixed/private", 2),
            ("mixed/public", 3),
        ] {
            std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
            for i in 0..files {
                std::fs::write(
                    tmp.path().join(dir).join(format!("f{i}")),
                    vec![7u8; 5000 * (i + 1)],
                )
                .unwrap();
            }
        }
        std::fs::write(tmp.path().join("big.bin"), vec![1u8; 300_000]).unwrap();
        tmp
    }

    fn scan_with(root: &Path, excludes: &[PathBuf]) -> ScanTree {
        let mut opts = ScanOptions::new(root);
        opts.excludes = excludes.to_vec();
        opts.min_file_bytes = 1;
        scan(&opts, &NoClassify, &ScanControl::new()).unwrap()
    }

    /// Every folder by path, with its totals and flags.
    fn summary(t: &ScanTree) -> Vec<(PathBuf, u64, u64, u64, u64, bool)> {
        let mut v: Vec<_> = (0..t.dirs.len() as u32)
            .map(|i| {
                let d = &t.dirs[i as usize];
                (
                    t.dir_path(i),
                    d.alloc_bytes,
                    d.logical_bytes,
                    d.file_count,
                    d.dir_count,
                    d.flags.excluded,
                )
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn removing_after_the_scan_equals_skipping_during_the_walk() {
        let tmp = tree_on_disk();
        let excludes = vec![tmp.path().join("secret"), tmp.path().join("mixed/private")];
        let skipped = scan_with(tmp.path(), &excludes);
        let mut removed = scan_with(tmp.path(), &[]);
        apply(&mut removed, &excludes);
        assert_eq!(summary(&removed), summary(&skipped));
        assert_eq!(removed.root().kind_bytes, skipped.root().kind_bytes);
        // Nothing from inside an excluded folder is kept, not even its files.
        let mut files: Vec<_> = removed
            .files
            .iter()
            .map(|f| removed.dir_path(f.dir).join(&f.name))
            .collect();
        let mut want: Vec<_> = skipped
            .files
            .iter()
            .map(|f| skipped.dir_path(f.dir).join(&f.name))
            .collect();
        files.sort();
        want.sort();
        assert_eq!(files, want);
        assert!(files
            .iter()
            .all(|p| !p.starts_with(tmp.path().join("secret"))));
    }

    #[test]
    fn the_result_is_still_a_well_formed_tree() {
        let tmp = tree_on_disk();
        let mut t = scan_with(tmp.path(), &[]);
        apply(
            &mut t,
            &[tmp.path().join("secret"), tmp.path().join("secret/deeper")],
        );
        for (i, d) in t.dirs.iter().enumerate() {
            if let Some(p) = d.parent {
                assert!((p as usize) < i, "parents come first");
                assert!(t.dirs[p as usize].children.contains(&(i as u32)));
            }
            let below: u64 = d
                .children
                .iter()
                .map(|&c| t.dirs[c as usize].alloc_bytes)
                .sum();
            assert!(d.alloc_bytes >= below + d.own_files_alloc);
        }
        assert!(t.files.iter().all(|f| (f.dir as usize) < t.dirs.len()));
    }

    #[test]
    fn nothing_to_exclude_changes_nothing() {
        let tmp = tree_on_disk();
        let before = scan_with(tmp.path(), &[]);
        let mut after = before.clone();
        apply(
            &mut after,
            &[tmp.path().join("not-there"), tmp.path().to_path_buf()],
        );
        assert_eq!(summary(&after), summary(&before));
    }
}
