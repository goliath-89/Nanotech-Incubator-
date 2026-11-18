//! Acting on saved cleanup suggestions.
//!
//! The window can only name suggestions by number. Paths come from the
//! scan history, never from the window, and every item goes through
//! `strata_safety::trash`, which checks it again at its real location.

use std::path::PathBuf;
use strata_reclaim::{expand, Target};
use strata_safety::{trash, Policy};
use strata_store::{ScanId, Store, StoreError};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanupReport {
    /// Suggestions whose every item is now in the Trash.
    pub done: Vec<u32>,
    /// Bytes those suggestions held at scan time.
    pub bytes_freed: u64,
    /// Files those suggestions held when they were moved (a folder counts
    /// the files inside it; folders themselves are not counted).
    pub files_moved: u64,
    /// Items the safety policy refused, with the reason in plain words.
    pub refused: Vec<(PathBuf, String)>,
    /// Items that could not be moved, with what went wrong.
    pub failed: Vec<(PathBuf, String)>,
}

/// Move the items of suggestions `numbers` of scan `scan` to the Trash.
/// Unknown numbers are ignored. Suggestions fully done are forgotten.
pub fn clean_up(
    store: &mut Store,
    scan: ScanId,
    numbers: &[u32],
    policy: &Policy,
) -> Result<CleanupReport, StoreError> {
    let root = store.scan_info(scan)?.root;
    let saved = store.numbered_suggestions(scan)?;
    let mut report = CleanupReport::default();

    for (n, row) in saved.into_iter().filter(|(n, _)| numbers.contains(n)) {
        let target = if row.contents_only {
            Target::Contents(row.path.clone())
        } else {
            Target::Item(row.path.clone())
        };
        let items = match expand(&target) {
            Ok(items) => items,
            Err(e) => {
                report.failed.push((row.path, e.to_string()));
                continue;
            }
        };
        // Counted before the move: afterwards the files are in the Trash.
        let files: u64 = items.iter().map(|p| count_files(p)).sum();
        let result = trash(policy, &root, &items);
        let clean = result.refused.is_empty() && result.failed.is_empty();
        report
            .refused
            .extend(result.refused.into_iter().map(|(p, r)| (p, r.to_string())));
        report.failed.extend(result.failed);
        if clean {
            report.done.push(n);
            report.bytes_freed += row.bytes;
            report.files_moved += files;
        }
    }
    store.remove_suggestions(scan, &report.done)?;
    Ok(report)
}

/// Files at or below `path`, without following links (a link counts as
/// one file). Unreadable folders count as nothing.
fn count_files(path: &std::path::Path) -> u64 {
    let mut count = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if !meta.is_dir() {
            count += 1;
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&p) {
            stack.extend(entries.flatten().map(|e| e.path()));
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{ScanControl, ScanOptions};
    use strata_store::{ScanMeta, SuggestionRow};

    fn row(path: PathBuf, contents_only: bool, bytes: u64) -> SuggestionRow {
        SuggestionRow {
            group: "cache".into(),
            contents_only,
            path,
            bytes,
            app: None,
            rebuildable: true,
            reason: "test".into(),
        }
    }

    #[test]
    fn moves_only_named_saved_suggestions_and_forgets_them() {
        // A scratch home inside the build folder (not under a `tmp` folder).
        let build = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::create_dir_all(&build).unwrap();
        let build = build.canonicalize().unwrap(); // the policy refuses paths with `..`
        let tmp = tempfile::tempdir_in(&build).unwrap();
        let home = tmp.path().join("people/sam");
        let cache = home.join("app-cache");
        std::fs::create_dir_all(cache.join("sub")).unwrap();
        std::fs::write(cache.join("a.bin"), vec![1u8; 5000]).unwrap();
        std::fs::write(cache.join("sub/b.bin"), vec![1u8; 5000]).unwrap();
        let old = home.join("old-installer.dmg");
        std::fs::write(&old, vec![1u8; 5000]).unwrap();
        let keep = home.join("keep.txt");
        std::fs::write(&keep, b"keep").unwrap();

        let tree = strata_core::scan(
            &ScanOptions::new(&home),
            &strata_core::NoClassify,
            &ScanControl::new(),
        )
        .unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let scan = store
            .save(
                &tree,
                &ScanMeta {
                    volume_id: "v".into(),
                    started_at: 0,
                    finished_at: 0,
                },
            )
            .unwrap();
        store
            .save_suggestions(
                scan,
                &[
                    row(cache.clone(), true, 10_000),
                    row(old.clone(), false, 5_000),
                    row(keep.clone(), false, 4),
                ],
            )
            .unwrap();

        let home_text = home.to_string_lossy().into_owned();
        let policy = Policy::new(strata_safety::Platform::current(), &[&home_text]).unwrap();
        // Number 2 (keep.txt) is not asked for; 99 does not exist.
        let report = clean_up(&mut store, scan, &[0, 1, 99], &policy).unwrap();
        assert_eq!(report.done, [0, 1], "{report:?}");
        assert_eq!(report.bytes_freed, 15_000);
        assert_eq!(report.files_moved, 3, "a.bin, sub/b.bin and the installer");
        assert!(report.refused.is_empty() && report.failed.is_empty());

        assert!(
            cache.is_dir(),
            "a contents-only suggestion keeps the folder"
        );
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
        assert!(!old.exists());
        assert!(keep.exists());
        let left: Vec<u32> = store
            .numbered_suggestions(scan)
            .unwrap()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            left,
            [2],
            "done suggestions are forgotten, the rest keep their numbers"
        );

        // A saved suggestion that points somewhere protected is refused at
        // the last step, whatever the history says.
        let mut strict = policy.clone();
        strict.protect(&keep.to_string_lossy()).unwrap();
        let report = clean_up(&mut store, scan, &[2], &strict).unwrap();
        assert!(report.done.is_empty());
        assert_eq!(report.refused.len(), 1);
        assert!(keep.exists());
    }
}
