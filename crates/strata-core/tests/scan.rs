//! Scanner tests against generated folder trees with known contents.
#![cfg(unix)]

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use strata_core::{scan, Classify, Kind, NoClassify, ScanControl, ScanOptions, ScanTree};

fn write(path: &Path, bytes: usize) {
    let mut f = File::create(path).unwrap();
    f.write_all(&vec![0xA5u8; bytes]).unwrap();
    f.sync_all().unwrap();
}

fn run(root: &Path) -> ScanTree {
    scan(&ScanOptions::new(root), &NoClassify, &ScanControl::new()).unwrap()
}

/// Deliberately simple single-threaded walk, written independently of the
/// scanner, that the scanner's totals must match.
#[derive(Default, Debug, PartialEq)]
struct Reference {
    alloc: u64,
    files: u64,
    dirs: u64,
}

fn reference(root: &Path) -> Reference {
    fn walk(dir: &Path, seen: &mut HashSet<(u64, u64)>, r: &mut Reference) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let md = fs::symlink_metadata(&path).unwrap();
            if md.is_dir() {
                r.dirs += 1;
                r.alloc += md.blocks() * 512;
                walk(&path, seen, r);
            } else {
                r.files += 1;
                if md.nlink() == 1 || seen.insert((md.dev(), md.ino())) {
                    r.alloc += md.blocks() * 512;
                }
            }
        }
    }
    let mut r = Reference {
        alloc: fs::metadata(root).unwrap().blocks() * 512,
        ..Default::default()
    };
    walk(root, &mut HashSet::new(), &mut r);
    r
}

fn scanned(tree: &ScanTree) -> Reference {
    Reference {
        alloc: tree.root().alloc_bytes,
        files: tree.root().file_count,
        dirs: tree.root().dir_count,
    }
}

/// A few hundred folders and a few thousand files of mixed sizes, with hard
/// links and symbolic links sprinkled in. Same tree every run.
fn build_mixed_tree(root: &Path) {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut dirs = vec![root.to_path_buf()];
    for i in 0..300 {
        let parent = dirs[(next() % dirs.len() as u64) as usize].clone();
        let d = parent.join(format!("dir{i}"));
        fs::create_dir(&d).unwrap();
        dirs.push(d);
    }
    let mut files = Vec::new();
    for i in 0..3000 {
        let d = &dirs[(next() % dirs.len() as u64) as usize];
        let size = match next() % 10 {
            0 => 0,
            1..=6 => next() % 4096,
            7 | 8 => next() % 200_000,
            _ => 1_000_000 + next() % 2_000_000,
        };
        let f = d.join(format!("file{i}.bin"));
        write(&f, size as usize);
        files.push(f);
    }
    for i in 0..200 {
        let target = &files[(next() % files.len() as u64) as usize];
        let d = &dirs[(next() % dirs.len() as u64) as usize];
        fs::hard_link(target, d.join(format!("link{i}"))).unwrap();
    }
    for i in 0..50 {
        let target = &dirs[(next() % dirs.len() as u64) as usize];
        let d = &dirs[(next() % dirs.len() as u64) as usize];
        std::os::unix::fs::symlink(target, d.join(format!("sym{i}"))).unwrap();
    }
}

#[test]
fn totals_match_an_independent_walk() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let tree = run(tmp.path());
    assert_eq!(scanned(&tree), reference(tmp.path()));
    assert_eq!(tree.stats.error_count, 0);
}

#[cfg(target_os = "linux")]
#[test]
fn total_matches_du() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let out = std::process::Command::new("du")
        .args(["-s", "-B1"])
        .arg(tmp.path())
        .output();
    let Ok(out) = out else { return }; // no `du` on this machine
    let text = String::from_utf8_lossy(&out.stdout);
    let du: u64 = text.split_whitespace().next().unwrap().parse().unwrap();
    assert_eq!(run(tmp.path()).root().alloc_bytes, du);
}

#[test]
fn folder_totals_are_consistent() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let tree = run(tmp.path());

    for (i, d) in tree.dirs.iter().enumerate() {
        if let Some(p) = d.parent {
            assert!((p as usize) < i, "parents come before children");
        }
        let below: u64 = d
            .children
            .iter()
            .map(|&c| tree.dirs[c as usize].alloc_bytes)
            .sum();
        assert!(d.alloc_bytes >= below + d.own_files_alloc);
        let files_below: u64 = d
            .children
            .iter()
            .map(|&c| tree.dirs[c as usize].file_count)
            .sum();
        assert_eq!(d.file_count, files_below + d.own_file_count);
        let names: Vec<_> = d
            .children
            .iter()
            .map(|&c| &tree.dirs[c as usize].name)
            .collect();
        assert!(
            names.windows(2).all(|w| w[0] < w[1]),
            "children sorted by name"
        );
    }
    // Every file byte is in exactly one kind bucket.
    let own_files: u64 = tree.dirs.iter().map(|d| d.own_files_alloc).sum();
    assert_eq!(tree.root().kind_bytes.iter().sum::<u64>(), own_files);
}

#[test]
fn results_are_identical_across_runs_and_thread_counts() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let fingerprint = |threads: usize| {
        let mut opts = ScanOptions::new(tmp.path());
        opts.threads = threads;
        let t = scan(&opts, &NoClassify, &ScanControl::new()).unwrap();
        let dirs: Vec<_> = t
            .dirs
            .iter()
            .map(|d| (d.name.clone(), d.alloc_bytes, d.file_count))
            .collect();
        let files: Vec<_> = t
            .files
            .iter()
            .map(|f| (f.dir, f.name.clone(), f.alloc_bytes))
            .collect();
        (
            dirs,
            files,
            t.stats.hardlink_duplicates,
            t.stats.hardlink_saved_bytes,
        )
    };
    let first = fingerprint(1);
    for threads in [2, 8, 8, 8] {
        assert_eq!(fingerprint(threads), first);
    }
}

#[test]
fn hard_linked_file_is_counted_once_in_the_first_folder() {
    let tmp = tempfile::tempdir().unwrap();
    for d in ["a", "b", "c"] {
        fs::create_dir(tmp.path().join(d)).unwrap();
    }
    let original = tmp.path().join("b/movie.bin");
    write(&original, 2_000_000);
    fs::hard_link(&original, tmp.path().join("a/z-link.bin")).unwrap();
    fs::hard_link(&original, tmp.path().join("c/link.bin")).unwrap();
    let size = fs::metadata(&original).unwrap().blocks() * 512;

    let tree = run(tmp.path());
    let dir = |name: &str| {
        tree.dirs
            .iter()
            .find(|d| d.name == OsStr::new(name))
            .unwrap()
    };
    assert_eq!(
        dir("a").own_files_alloc,
        size,
        "first folder in tree order owns the bytes"
    );
    assert_eq!(dir("b").own_files_alloc, 0);
    assert_eq!(dir("c").own_files_alloc, 0);
    assert_eq!(
        tree.root().file_count,
        3,
        "every link is still listed as an entry"
    );
    assert_eq!(tree.stats.hardlink_duplicates, 2);
    assert_eq!(tree.stats.hardlink_saved_bytes, 2 * size);
    assert_eq!(tree.files.len(), 1, "listed once among the largest files");
    assert_eq!(
        tree.file_path(&tree.files[0]),
        tmp.path().join("a/z-link.bin")
    );
    assert_eq!(scanned(&tree), reference(tmp.path()));
}

#[test]
fn symbolic_links_are_not_followed() {
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("huge.bin"), 3_000_000);
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    // A loop back to the root, and a link out to a big folder elsewhere.
    std::os::unix::fs::symlink(tmp.path(), tmp.path().join("sub/loop")).unwrap();
    std::os::unix::fs::symlink(outside.path(), tmp.path().join("elsewhere")).unwrap();
    std::os::unix::fs::symlink("/does/not/exist", tmp.path().join("dangling")).unwrap();

    let tree = run(tmp.path());
    assert_eq!(tree.root().dir_count, 1);
    assert_eq!(
        tree.root().file_count,
        3,
        "links count as entries, not as their targets"
    );
    assert!(tree.root().alloc_bytes < 1_000_000);
    assert_eq!(tree.stats.error_count, 0);
}

#[test]
fn sparse_file_counts_what_is_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let f = File::create(tmp.path().join("sparse.img")).unwrap();
    f.set_len(200 * 1024 * 1024).unwrap();
    drop(f);
    let on_disk = fs::metadata(tmp.path().join("sparse.img"))
        .unwrap()
        .blocks()
        * 512;
    if on_disk >= 200 * 1024 * 1024 {
        return; // this file system does not do sparse files
    }
    let tree = run(tmp.path());
    assert_eq!(tree.root().logical_bytes, 200 * 1024 * 1024);
    assert_eq!(tree.root().own_files_alloc, on_disk);
}

#[test]
fn odd_names_survive() {
    let tmp = tempfile::tempdir().unwrap();
    let names: Vec<OsString> = vec![
        OsString::from("日本語 フォルダ"),
        OsString::from("emoji 🗂️"),
        OsString::from(" leading space"),
        OsString::from("new\nline"),
        OsStr::from_bytes(&[b'b', b'a', b'd', 0xFF, 0xFE]).to_os_string(), // not UTF-8
    ];
    for n in &names {
        match fs::create_dir(tmp.path().join(n)) {
            Ok(()) => write(&tmp.path().join(n).join("f"), 1_500_000),
            Err(_) if n.to_str().is_none() => {} // file system insists on UTF-8 names
            Err(e) => panic!("{e}"),
        }
    }
    let tree = run(tmp.path());
    assert_eq!(scanned(&tree), reference(tmp.path()));
    for f in &tree.files {
        assert!(
            tree.file_path(f).exists(),
            "rebuilt path points at the real file"
        );
    }
}

#[test]
fn very_deep_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let mut p = tmp.path().to_path_buf();
    // Stay under the 4096-byte path limit: each level adds two bytes.
    for _ in 0..1500 {
        p.push("d");
    }
    if fs::create_dir_all(&p).is_err() {
        return;
    }
    write(&p.join("bottom.bin"), 1_200_000);
    let tree = run(tmp.path());
    assert_eq!(tree.root().dir_count, 1500);
    assert_eq!(tree.root().file_count, 1);
    assert_eq!(tree.file_path(&tree.files[0]), p.join("bottom.bin"));
}

#[test]
fn unreadable_folder_is_reported_not_dropped() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    write(&locked.join("secret.bin"), 10_000);
    write(&tmp.path().join("open.bin"), 10_000);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let blocked = fs::read_dir(&locked).is_err(); // root can read anything
    let tree = run(tmp.path());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !blocked {
        return;
    }
    let d = tree
        .dirs
        .iter()
        .find(|d| d.name == OsStr::new("locked"))
        .unwrap();
    assert!(d.flags.unreadable && d.flags.not_scanned());
    assert_eq!(tree.stats.error_count, 1);
    assert_eq!(tree.errors[0].path, locked);
    assert_eq!(
        tree.errors[0].kind,
        strata_core::ScanErrorKind::PermissionDenied
    );
    assert_eq!(
        tree.root().file_count,
        1,
        "the readable file is still counted"
    );
}

#[test]
fn largest_files_list_respects_threshold_and_cap() {
    let tmp = tempfile::tempdir().unwrap();
    for (i, size) in [5_000_000, 3_000_000, 4_000_000, 2_000_000, 10_000]
        .iter()
        .enumerate()
    {
        write(&tmp.path().join(format!("f{i}")), *size);
    }
    let mut opts = ScanOptions::new(tmp.path());
    opts.min_file_bytes = 1_000_000;
    let tree = scan(&opts, &NoClassify, &ScanControl::new()).unwrap();
    let names: Vec<_> = tree
        .files
        .iter()
        .map(|f| f.name.to_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["f0", "f2", "f1", "f3"],
        "biggest first, small file left out"
    );

    opts.max_files = 2;
    let tree = scan(&opts, &NoClassify, &ScanControl::new()).unwrap();
    let names: Vec<_> = tree
        .files
        .iter()
        .map(|f| f.name.to_str().unwrap())
        .collect();
    assert_eq!(names, ["f0", "f2"]);
    assert_eq!(
        tree.root().file_count,
        5,
        "the cap never changes the totals"
    );
}

#[test]
fn excluded_folder_is_not_entered() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("keep")).unwrap();
    fs::create_dir_all(tmp.path().join("skip/inner")).unwrap();
    write(&tmp.path().join("keep/a"), 50_000);
    write(&tmp.path().join("skip/inner/b"), 50_000);
    let mut opts = ScanOptions::new(tmp.path());
    opts.excludes = vec![tmp.path().join("skip")];
    let tree = scan(&opts, &NoClassify, &ScanControl::new()).unwrap();
    assert_eq!(tree.root().file_count, 1);
    let skip = tree
        .dirs
        .iter()
        .find(|d| d.name == OsStr::new("skip"))
        .unwrap();
    assert!(skip.flags.excluded && skip.children.is_empty());
}

#[test]
fn cancelled_scan_returns_partial_result() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let ctl = ScanControl::new();
    ctl.cancel();
    let tree = scan(&ScanOptions::new(tmp.path()), &NoClassify, &ctl).unwrap();
    assert!(tree.stats.cancelled);
    assert!(tree.root().flags.cancelled);
    assert_eq!(tree.root().file_count, 0);
}

#[test]
fn progress_counters_reach_the_totals() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let ctl = ScanControl::new();
    let tree = scan(&ScanOptions::new(tmp.path()), &NoClassify, &ctl).unwrap();
    assert_eq!(ctl.files(), tree.root().file_count);
    assert_eq!(ctl.dirs(), tree.root().dir_count + 1);
    assert!(ctl.bytes() <= tree.root().alloc_bytes);
}

#[test]
fn classifier_decides_kind_totals() {
    struct ByExtension;
    impl Classify for ByExtension {
        fn folder_kind(&self, dir: &Path) -> Option<Kind> {
            (dir.file_name()? == "Caches").then_some(Kind::Caches)
        }
        fn file_kind(&self, name: &OsStr) -> Kind {
            match Path::new(name).extension().and_then(|e| e.to_str()) {
                Some("mp4") => Kind::Videos,
                Some("jpg") => Kind::Photos,
                _ => Kind::Other,
            }
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    write(&tmp.path().join("a.mp4"), 300_000);
    write(&tmp.path().join("sub/b.mp4"), 200_000);
    write(&tmp.path().join("sub/c.jpg"), 100_000);
    write(&tmp.path().join("notes.txt"), 5_000);
    fs::create_dir(tmp.path().join("Caches")).unwrap();
    write(&tmp.path().join("Caches/thumb.jpg"), 40_000);
    let on_disk = |p: &str| fs::metadata(tmp.path().join(p)).unwrap().blocks() * 512;

    let tree = scan(
        &ScanOptions::new(tmp.path()),
        &ByExtension,
        &ScanControl::new(),
    )
    .unwrap();
    let kinds = tree.root().kind_bytes;
    assert_eq!(
        kinds[Kind::Videos as usize],
        on_disk("a.mp4") + on_disk("sub/b.mp4")
    );
    assert_eq!(kinds[Kind::Photos as usize], on_disk("sub/c.jpg"));
    assert_eq!(kinds[Kind::Other as usize], on_disk("notes.txt"));
    assert_eq!(
        kinds[Kind::Caches as usize],
        on_disk("Caches/thumb.jpg"),
        "the folder's kind wins over the name"
    );
    assert_eq!(tree.files.len(), 0);
}

#[test]
fn bad_roots_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    write(&tmp.path().join("file"), 10);
    let go = |p: &Path| scan(&ScanOptions::new(p), &NoClassify, &ScanControl::new());
    assert!(go(&tmp.path().join("missing")).is_err());
    assert!(go(&tmp.path().join("file")).is_err());
}

/// A drive in use changes under the scanner. Files that vanish between
/// being listed and being examined are not errors, and nothing may panic.
#[test]
fn a_tree_that_changes_during_the_scan_is_handled_quietly() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        // Keep creating and deleting folders full of files while scans run.
        scope.spawn(|| {
            let mut round = 0u32;
            while !stop.load(Ordering::Relaxed) {
                let churn = tmp.path().join(format!("churn{}", round % 4));
                let _ = fs::create_dir_all(churn.join("inner"));
                for i in 0..200 {
                    let _ = fs::write(churn.join(format!("f{i}")), b"x");
                    let _ = fs::write(churn.join("inner").join(format!("g{i}")), b"x");
                }
                let _ = fs::remove_dir_all(&churn);
                round += 1;
            }
        });
        for _ in 0..40 {
            let tree = run(tmp.path());
            assert_eq!(tree.stats.error_count, 0, "{:?}", tree.errors.first());
            assert!(tree.root().file_count >= 3200);
        }
        stop.store(true, Ordering::Relaxed);
    });
}

#[test]
fn running_totals_per_top_folder_end_at_the_final_totals() {
    let tmp = tempfile::tempdir().unwrap();
    build_mixed_tree(tmp.path());
    let ctl = ScanControl::new();
    assert!(ctl.top_level_so_far().is_empty(), "nothing before the scan");
    let tree = scan(&ScanOptions::new(tmp.path()), &NoClassify, &ctl).unwrap();
    let so_far = ctl.top_level_so_far();
    let root = tree.root();
    assert_eq!(so_far.len(), root.children.len());
    for &c in &root.children {
        let d = &tree.dirs[c as usize];
        let t = so_far
            .iter()
            .find(|t| t.name == d.name)
            .expect("every top folder");
        // Hard-linked files are placed after the walk, so the running total
        // can only be short by what hard links added.
        assert!(
            t.bytes <= d.alloc_bytes,
            "{:?}: {} > {}",
            d.name,
            t.bytes,
            d.alloc_bytes
        );
    }
}

#[test]
fn running_totals_match_exactly_without_hard_links() {
    let tmp = tempfile::tempdir().unwrap();
    for (dir, files) in [("a", 3), ("b/c", 5), ("b/d", 2), ("e", 0)] {
        fs::create_dir_all(tmp.path().join(dir)).unwrap();
        for i in 0..files {
            write(&tmp.path().join(dir).join(format!("f{i}")), 3000 * (i + 1));
        }
    }
    let ctl = ScanControl::new();
    let tree = scan(&ScanOptions::new(tmp.path()), &NoClassify, &ctl).unwrap();
    let mut so_far = ctl.top_level_so_far();
    so_far.sort_by(|x, y| x.name.cmp(&y.name));
    let finals: Vec<(OsString, u64)> = tree
        .root()
        .children
        .iter()
        .map(|&c| {
            (
                tree.dirs[c as usize].name.clone(),
                tree.dirs[c as usize].alloc_bytes,
            )
        })
        .collect();
    let running: Vec<(OsString, u64)> = so_far.into_iter().map(|t| (t.name, t.bytes)).collect();
    assert_eq!(running, finals);
}
