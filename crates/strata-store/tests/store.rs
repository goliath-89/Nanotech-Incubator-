use std::ffi::OsString;
use std::fs;
use std::path::Path;
use strata_core::{
    scan, DirFlags, DirNode, Kind, NoClassify, ScanControl, ScanOptions, ScanTree, KIND_COUNT,
};
use strata_store::{ScanMeta, Store, StoreError, SNAPSHOT_MIN_BYTES};

fn meta(volume: &str, finished_at: i64) -> ScanMeta {
    ScanMeta {
        volume_id: volume.into(),
        started_at: finished_at - 10,
        finished_at,
    }
}

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![7u8; bytes]).unwrap();
}

/// A small real tree on disk, scanned for real.
fn real_scan(root: &Path) -> ScanTree {
    write(&root.join("Movies/holiday.mp4"), 3_000_000);
    write(&root.join("Movies/clips/a.mov"), 1_500_000);
    write(&root.join("Documents/notes.txt"), 4_000);
    write(&root.join("Documents/report.pdf"), 2_000_000);
    fs::create_dir_all(root.join("Empty")).unwrap();
    scan(&ScanOptions::new(root), &NoClassify, &ScanControl::new()).unwrap()
}

fn assert_same(a: &ScanTree, b: &ScanTree) {
    assert_eq!(a.dirs, b.dirs);
    assert_eq!(a.files, b.files);
    assert_eq!(a.errors, b.errors);
    assert_eq!(a.stats.error_count, b.stats.error_count);
    assert_eq!(a.stats.cancelled, b.stats.cancelled);
    assert_eq!(a.stats.elapsed_ms, b.stats.elapsed_ms);
    assert_eq!(a.stats.hardlink_duplicates, b.stats.hardlink_duplicates);
    assert_eq!(a.stats.hardlink_saved_bytes, b.stats.hardlink_saved_bytes);
}

/// A hand-built tree for tests that need exact numbers. `spec` is
/// (parent index or None, name, bytes of files directly inside).
fn tree(spec: &[(Option<u32>, &str, u64)]) -> ScanTree {
    let mut dirs: Vec<DirNode> = spec
        .iter()
        .map(|(parent, name, own)| DirNode {
            parent: *parent,
            name: OsString::from(name),
            children: Vec::new(),
            alloc_bytes: *own,
            logical_bytes: *own,
            file_count: 1,
            dir_count: 0,
            kind_bytes: [0; KIND_COUNT],
            cloud_only_bytes: 0,
            own_files_alloc: *own,
            own_file_count: 1,
            flags: DirFlags::default(),
        })
        .collect();
    for i in (1..dirs.len()).rev() {
        let p = dirs[i].parent.unwrap() as usize;
        dirs[p].alloc_bytes += dirs[i].alloc_bytes;
        dirs[p].dir_count += dirs[i].dir_count + 1;
    }
    for i in 1..dirs.len() {
        let p = dirs[i].parent.unwrap() as usize;
        dirs[p].children.push(i as u32);
    }
    ScanTree {
        dirs,
        files: Vec::new(),
        errors: Vec::new(),
        stats: Default::default(),
    }
}

const MB: u64 = 1024 * 1024;

#[test]
fn a_scan_round_trips_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let original = real_scan(tmp.path());
    let mut store = Store::open_in_memory().unwrap();
    let id = store.save(&original, &meta("vol", 1000)).unwrap();
    assert_same(&store.load(id).unwrap(), &original);

    let info = store.scan_info(id).unwrap();
    assert_eq!(info.root, tmp.path());
    assert_eq!(info.alloc_bytes, original.root().alloc_bytes);
    assert_eq!(info.file_count, 4);
    assert!(info.has_tree);
}

#[test]
fn survives_closing_and_reopening_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let original = real_scan(&tmp.path().join("data"));
    let db = tmp.path().join("history.sqlite");
    let id = {
        let mut store = Store::open(&db).unwrap();
        store.save(&original, &meta("vol", 1000)).unwrap()
    };
    let store = Store::open(&db).unwrap();
    assert_same(&store.load(id).unwrap(), &original);
}

#[cfg(unix)]
#[test]
fn names_that_are_not_text_come_back_byte_for_byte() {
    use std::os::unix::ffi::OsStringExt;
    let odd = OsString::from_vec(vec![b'x', 0xFF, 0xFE, b'/', 0x80]);
    let mut t = tree(&[(None, "/root", 0), (Some(0), "placeholder", 5 * MB)]);
    t.dirs[1].name = odd.clone();
    let mut store = Store::open_in_memory().unwrap();
    let id = store.save(&t, &meta("vol", 1)).unwrap();
    assert_eq!(store.load(id).unwrap().dirs[1].name, odd);
}

#[test]
fn flags_errors_and_kinds_are_kept() {
    let mut t = tree(&[
        (None, "/root", 0),
        (Some(0), "locked", 0),
        (Some(0), "other", 0),
    ]);
    t.dirs[1].flags.unreadable = true;
    t.dirs[2].flags.other_filesystem = true;
    t.dirs[2].flags.excluded = true;
    t.dirs[0].kind_bytes[Kind::Videos as usize] = 123;
    t.dirs[0].kind_bytes[Kind::Caches as usize] = u64::MAX / 4;
    t.dirs[0].cloud_only_bytes = 77;
    t.errors.push(strata_core::ScanError {
        path: "/root/locked".into(),
        kind: strata_core::ScanErrorKind::PermissionDenied,
        message: "Permission denied".into(),
    });
    t.stats.error_count = 5; // more happened than were recorded
    t.stats.cancelled = true;
    t.stats.hardlink_duplicates = 3;
    t.stats.hardlink_saved_bytes = 9000;
    let mut store = Store::open_in_memory().unwrap();
    let id = store.save(&t, &meta("vol", 1)).unwrap();
    assert_same(&store.load(id).unwrap(), &t);
    assert_eq!(store.errors(id).unwrap(), t.errors);
}

#[test]
fn children_lists_one_level_largest_first_with_breadcrumb() {
    let tmp = tempfile::tempdir().unwrap();
    let original = real_scan(tmp.path());
    let mut store = Store::open_in_memory().unwrap();
    let id = store.save(&original, &meta("vol", 1000)).unwrap();

    let top = store.children(id, 0).unwrap();
    let names: Vec<_> = top
        .children
        .iter()
        .map(|d| d.name.to_str().unwrap())
        .collect();
    assert_eq!(names, ["Movies", "Documents", "Empty"]);
    assert_eq!(top.breadcrumb.len(), 1);
    assert!(top.files.is_empty());

    let movies = top.children[0].id;
    let listing = store.children(id, movies).unwrap();
    assert_eq!(listing.dir.name, "Movies");
    assert_eq!(listing.children.len(), 1);
    assert_eq!(listing.files.len(), 1);
    assert_eq!(listing.files[0].name, "holiday.mp4");

    let clips = listing.children[0].id;
    let deep = store.children(id, clips).unwrap();
    let crumb: Vec<_> = deep.breadcrumb.iter().map(|(_, n)| n.clone()).collect();
    assert_eq!(
        crumb,
        [tmp.path().as_os_str(), "Movies".as_ref(), "clips".as_ref()]
    );
    assert_eq!(
        store.dir_path(id, clips).unwrap(),
        tmp.path().join("Movies/clips")
    );
    assert!(matches!(
        store.children(id, 9999),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn largest_files_can_be_filtered_by_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let mut t = real_scan(tmp.path());
    for f in &mut t.files {
        if f.name.to_str().unwrap().ends_with(".pdf") {
            f.kind = Kind::Documents;
        } else {
            f.kind = Kind::Videos;
        }
    }
    let mut store = Store::open_in_memory().unwrap();
    let id = store.save(&t, &meta("vol", 1)).unwrap();

    let all = store.largest_files(id, None, 10).unwrap();
    let names: Vec<_> = all.iter().map(|f| f.name.to_str().unwrap()).collect();
    assert_eq!(names, ["holiday.mp4", "report.pdf", "a.mov"]);
    assert_eq!(store.largest_files(id, None, 2).unwrap().len(), 2);
    let videos = store.largest_files(id, Some(Kind::Videos), 10).unwrap();
    assert_eq!(videos.len(), 2);
    assert!(videos.iter().all(|f| f.kind == Kind::Videos));
}

#[test]
fn scans_are_listed_per_volume_newest_first() {
    let t = tree(&[(None, "/", 10 * MB)]);
    let mut store = Store::open_in_memory().unwrap();
    let a1 = store.save(&t, &meta("disk-a", 100)).unwrap();
    let b1 = store.save(&t, &meta("disk-b", 200)).unwrap();
    let a2 = store.save(&t, &meta("disk-a", 300)).unwrap();

    let ids = |v: Option<&str>| {
        store
            .scans(v)
            .unwrap()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(None), [a2, b1, a1]);
    assert_eq!(ids(Some("disk-a")), [a2, a1]);
    assert_eq!(store.latest("disk-b").unwrap().unwrap().id, b1);
    assert!(store.latest("nope").unwrap().is_none());
}

#[test]
fn diff_reports_growth_shrinkage_new_and_removed_folders() {
    let before = tree(&[
        (None, "/", 0),
        (Some(0), "Docker", 10 * MB),
        (Some(0), "Photos", 50 * MB),
        (Some(0), "OldGame", 30 * MB),
        (Some(0), "Same", 5 * MB),
    ]);
    let after = tree(&[
        (None, "/", 0),
        (Some(0), "Docker", 90 * MB),
        (Some(0), "Photos", 40 * MB),
        (Some(0), "NewApp", 20 * MB),
        (Some(0), "Same", 5 * MB),
    ]);
    let mut store = Store::open_in_memory().unwrap();
    let a = store.save(&before, &meta("vol", 1)).unwrap();
    let b = store.save(&after, &meta("vol", 2)).unwrap();

    let rows = store.diff(a, b, 100).unwrap();
    let got: Vec<(&str, i64)> = rows
        .iter()
        .map(|r| (r.path.as_str(), r.delta() / MB as i64))
        .collect();
    assert_eq!(
        got,
        [
            ("Docker", 80),
            ("", 60),
            ("OldGame", -30),
            ("NewApp", 20),
            ("Photos", -10)
        ]
    );
    assert_eq!(store.diff(a, b, 2).unwrap().len(), 2);
    assert!(store.diff(a, a, 100).unwrap().is_empty());
    assert!(matches!(store.diff(a, 999, 10), Err(StoreError::NotFound)));
}

#[test]
fn snapshot_skips_deep_and_tiny_folders() {
    let mut spec: Vec<(Option<u32>, String, u64)> = vec![(None, "/".into(), 0)];
    for depth in 1..=9u32 {
        spec.push((Some(depth - 1), format!("d{depth}"), 0));
    }
    spec.last_mut().unwrap().2 = 100 * MB; // all the bytes sit at the bottom
    spec.push((Some(0), "tiny".into(), SNAPSHOT_MIN_BYTES - 1));
    let spec: Vec<_> = spec.iter().map(|(p, n, b)| (*p, n.as_str(), *b)).collect();
    let t = tree(&spec);

    let mut store = Store::open_in_memory().unwrap();
    let a = store
        .save(&tree(&[(None, "/", 0)]), &meta("vol", 1))
        .unwrap();
    let b = store.save(&t, &meta("vol", 2)).unwrap();
    let paths: Vec<_> = store
        .diff(a, b, 100)
        .unwrap()
        .into_iter()
        .map(|r| (r.depth, r.path))
        .collect();
    assert_eq!(paths.len(), 7, "root plus six levels: {paths:?}");
    assert!(paths.contains(&(6, "d1/d2/d3/d4/d5/d6".to_string())));
    assert!(!paths.iter().any(|(_, p)| p == "tiny"));
}

#[test]
fn prune_keeps_totals_and_growth_but_drops_folders() {
    let mut store = Store::open_in_memory().unwrap();
    let mut ids = Vec::new();
    for i in 0..4u64 {
        let t = tree(&[(None, "/", 0), (Some(0), "Grows", (i + 1) * 10 * MB)]);
        ids.push(store.save(&t, &meta("vol", i as i64)).unwrap());
    }
    let other = store
        .save(&tree(&[(None, "/", MB)]), &meta("other-vol", 0))
        .unwrap();

    assert_eq!(store.prune("vol", 2).unwrap(), 2);
    assert_eq!(
        store.prune("vol", 2).unwrap(),
        0,
        "pruning twice changes nothing"
    );

    assert!(matches!(store.load(ids[0]), Err(StoreError::TreePruned)));
    assert!(matches!(
        store.children(ids[1], 0),
        Err(StoreError::TreePruned)
    ));
    assert!(store.load(ids[2]).is_ok());
    assert!(store.load(ids[3]).is_ok());
    assert!(store.load(other).is_ok(), "other volumes are untouched");

    let info = store.scan_info(ids[0]).unwrap();
    assert!(!info.has_tree);
    assert_eq!(info.alloc_bytes, 10 * MB);
    let growth = store.diff(ids[0], ids[3], 10).unwrap();
    let grows = growth
        .iter()
        .find(|r| r.path == "Grows")
        .expect("growth survives pruning");
    assert_eq!(grows.delta(), 30 * MB as i64);
}

#[test]
fn delete_removes_everything_about_a_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("h.sqlite");
    let mut store = Store::open(&db).unwrap();
    let keep = store
        .save(&tree(&[(None, "/", 5 * MB)]), &meta("vol", 1))
        .unwrap();
    let gone = store
        .save(&real_scan(&tmp.path().join("data")), &meta("vol", 2))
        .unwrap();
    store.delete(gone).unwrap();
    assert!(matches!(store.load(gone), Err(StoreError::NotFound)));
    assert!(matches!(store.delete(gone), Err(StoreError::NotFound)));
    assert!(store.load(keep).is_ok());
    drop(store);

    let raw = rusqlite::Connection::open(&db).unwrap();
    for table in ["dirs", "files", "scan_errors", "snapshots"] {
        let left: i64 = raw
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE scan_id = ?1"),
                [gone],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "{table} still has rows of the deleted scan");
    }
}

#[test]
fn a_save_that_fails_midway_leaves_nothing_behind() {
    let mut bad = tree(&[(None, "/", MB), (Some(0), "a", MB), (Some(0), "b", MB)]);
    bad.files.push(strata_core::FileNode {
        dir: 1,
        name: "f".into(),
        alloc_bytes: 1,
        logical_bytes: 1,
        kind: Kind::Other,
    });
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("h.sqlite");
    let mut store = Store::open(&db).unwrap();
    let good = store
        .save(&tree(&[(None, "/", MB)]), &meta("vol", 1))
        .unwrap();
    drop(store);

    // Make the files table reject the insert, so the save fails after the
    // scan row and all folder rows were already written.
    let raw = rusqlite::Connection::open(&db).unwrap();
    raw.execute_batch(
        "CREATE TRIGGER fail_files BEFORE INSERT ON files BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
    )
    .unwrap();
    drop(raw);

    let mut store = Store::open(&db).unwrap();
    assert!(matches!(
        store.save(&bad, &meta("vol", 2)),
        Err(StoreError::Sqlite(_))
    ));
    let scans = store.scans(None).unwrap();
    assert_eq!(scans.len(), 1, "the failed scan left no row behind");
    assert_eq!(scans[0].id, good);
    drop(store);
    let raw = rusqlite::Connection::open(&db).unwrap();
    let dirs: i64 = raw
        .query_row("SELECT COUNT(*) FROM dirs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dirs, 1, "no folder rows of the failed scan remain");
}

#[test]
fn a_history_from_a_newer_version_is_refused_not_damaged() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("h.sqlite");
    drop(Store::open(&db).unwrap());
    let raw = rusqlite::Connection::open(&db).unwrap();
    raw.pragma_update(None, "user_version", 99).unwrap();
    drop(raw);
    assert!(matches!(
        Store::open(&db),
        Err(StoreError::NewerSchema { found: 99, .. })
    ));
}

#[test]
fn large_scan_saves_and_reopens_quickly() {
    // 60,000 folders, three levels deep.
    let mut spec: Vec<(Option<u32>, String, u64)> = vec![(None, "/".into(), 0)];
    for a in 0..60u32 {
        spec.push((Some(0), format!("a{a:03}"), MB));
        let parent = spec.len() as u32 - 1;
        for b in 0..999u32 {
            spec.push((Some(parent), format!("b{b:04}"), 2 * MB));
        }
    }
    let spec: Vec<_> = spec.iter().map(|(p, n, b)| (*p, n.as_str(), *b)).collect();
    let t = tree(&spec);
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(&tmp.path().join("h.sqlite")).unwrap();

    let started = std::time::Instant::now();
    let id = store.save(&t, &meta("vol", 1)).unwrap();
    let saved = started.elapsed();
    let started = std::time::Instant::now();
    let top = store.children(id, 0).unwrap();
    let listed = started.elapsed();
    assert_eq!(top.children.len(), 60);
    assert_same(&store.load(id).unwrap(), &t);
    eprintln!("60k folders: save {saved:?}, list one level {listed:?}");
    assert!(
        listed.as_millis() < 200,
        "listing one level took {listed:?}"
    );
}
