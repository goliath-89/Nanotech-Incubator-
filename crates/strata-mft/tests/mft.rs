//! The file-table reader against a real NTFS volume image.
//!
//! `fixtures/small.img.gz` was built by `tools/make-ntfs-fixture.sh` with
//! ntfs-3g. `fixtures/small.expected.tsv` is what the mounted volume listed
//! at the time: an independent account of what the image holds.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use strata_classify::KindClassifier;
use strata_core::{Kind, NoClassify, ScanControl, ScanTree};
use strata_mft::{is_ntfs, scan_volume, MftError, MftOptions};

const ROOT: &str = "/vol";

fn image() -> Vec<u8> {
    let gz = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/small.img.gz"
    ))
    .unwrap();
    let mut img = Vec::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_end(&mut img)
        .unwrap();
    img
}

struct Entry {
    is_dir: bool,
    size: u64,
    blocks: u64,
    inode: u64,
    path: PathBuf,
}

struct Expected {
    entries: Vec<Entry>,
    used: u64,
}

fn expected() -> Expected {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/small.expected.tsv"
    ))
    .unwrap();
    let mut entries = Vec::new();
    let mut used = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f[0] == "used" {
            used = f[1].parse().unwrap();
            continue;
        }
        entries.push(Entry {
            is_dir: f[0] == "d",
            size: f[1].parse().unwrap(),
            blocks: f[2].parse().unwrap(),
            inode: f[3].parse().unwrap(),
            path: PathBuf::from(f[4]),
        });
    }
    Expected { entries, used }
}

/// Scan with every file kept individually, so each can be checked.
fn scan_all() -> ScanTree {
    let mut opts = MftOptions::new(ROOT);
    opts.min_file_bytes = 1;
    opts.max_files = usize::MAX;
    scan_volume(
        Cursor::new(image()),
        &opts,
        &KindClassifier,
        &ScanControl::new(),
    )
    .unwrap()
}

fn relative(tree: &ScanTree, dir: u32) -> PathBuf {
    tree.dir_path(dir).strip_prefix(ROOT).unwrap().to_path_buf()
}

/// NTFS keeps its own files at the top of the volume, all named `$...`.
fn is_ntfs_own(path: &Path) -> bool {
    path.components()
        .next()
        .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with('$'))
}

#[test]
fn total_matches_the_volumes_own_count_of_used_space() {
    let tree = scan_all();
    assert_eq!(
        tree.root().alloc_bytes,
        expected().used,
        "every used cluster is accounted for, and none twice"
    );
}

#[test]
fn folders_match_the_mounted_volume() {
    let tree = scan_all();
    let want = expected();
    let found: BTreeSet<PathBuf> = (1..tree.dirs.len() as u32)
        .map(|d| relative(&tree, d))
        .filter(|p| !is_ntfs_own(p))
        .collect();
    let listed: BTreeSet<PathBuf> = want
        .entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.clone())
        .collect();
    assert_eq!(found, listed);

    // Entries directly in each folder, hard-link names included.
    let mut per_folder: HashMap<PathBuf, u64> = HashMap::new();
    for e in want.entries.iter().filter(|e| !e.is_dir) {
        *per_folder
            .entry(e.path.parent().unwrap().to_path_buf())
            .or_default() += 1;
    }
    for d in 1..tree.dirs.len() as u32 {
        let path = relative(&tree, d);
        if !is_ntfs_own(&path) {
            let want = per_folder.get(&path).copied().unwrap_or(0);
            assert_eq!(
                tree.dirs[d as usize].own_file_count,
                want,
                "{}",
                path.display()
            );
        }
    }
    // The root also holds NTFS's own files ($MFT, $LogFile, ...).
    let root_files = tree.root().own_file_count;
    assert!(
        (10..=12).contains(&(root_files - per_folder[Path::new("")])),
        "root has {root_files} files"
    );
}

#[test]
fn every_file_has_the_right_length_and_space_on_disk() {
    let tree = scan_all();
    let want = expected();
    let found: BTreeMap<PathBuf, (u64, u64)> = tree
        .files
        .iter()
        .map(|f| {
            (
                tree.file_path(f).strip_prefix(ROOT).unwrap().to_path_buf(),
                (f.logical_bytes, f.alloc_bytes),
            )
        })
        .collect();
    let mut checked = 0;
    let mut seen_inodes = BTreeSet::new();
    for e in want.entries.iter().filter(|e| !e.is_dir) {
        // A file small enough to live inside its record owns no clusters;
        // the mounted volume reports such files as one block or none.
        if e.blocks <= 1 {
            assert!(
                !found.contains_key(&e.path),
                "{} should take no clusters",
                e.path.display()
            );
            continue;
        }
        if !seen_inodes.insert(e.inode) {
            continue; // a further name of a file already checked
        }
        let names: Vec<&Entry> = want.entries.iter().filter(|x| x.inode == e.inode).collect();
        let (length, on_disk) = names
            .iter()
            .find_map(|n| found.get(&n.path))
            .unwrap_or_else(|| panic!("{} is missing", e.path.display()));
        assert_eq!(*length, e.size, "length of {}", e.path.display());
        // The mounted volume rounds to 512-byte blocks; clusters here are 4096.
        assert_eq!(
            *on_disk,
            e.blocks * 512,
            "space on disk of {}",
            e.path.display()
        );
        checked += 1;
    }
    assert_eq!(checked, 9, "every file that owns clusters was compared");
}

#[test]
fn awkward_cases() {
    let tree = scan_all();
    let dir = |path: &str| {
        (0..tree.dirs.len() as u32)
            .find(|&d| relative(&tree, d) == Path::new(path))
            .map(|d| &tree.dirs[d as usize])
            .unwrap_or_else(|| panic!("{path} missing"))
    };

    // A sparse file counts what it occupies, not its length.
    assert_eq!(dir("sparse").logical_bytes, 6 * 1024 * 1024 + 4);
    assert_eq!(dir("sparse").alloc_bytes, 4096);

    // Three names, one file: counted once, in the first folder in order.
    assert_eq!(tree.stats.hardlink_duplicates, 2);
    assert_eq!(tree.stats.hardlink_saved_bytes, 2 * 200_704);
    assert_eq!(dir("hl").own_files_alloc, 200_704);
    assert_eq!(dir("hl").own_file_count, 2);
    assert_eq!(dir("other").own_files_alloc, 0);
    assert_eq!(dir("other").own_file_count, 1);

    // 1,500 tiny files live inside their records; the folder's index does not.
    let many = dir("many");
    assert_eq!(many.own_file_count, 1500);
    assert_eq!(many.own_files_alloc, 0);
    assert_eq!(many.logical_bytes, 1500 * 5);
    assert!(
        many.alloc_bytes > 100_000,
        "the folder's own index takes clusters: {}",
        many.alloc_bytes
    );

    // Names that are not ASCII, and a deep chain.
    assert_eq!(dir("Ünïcödé 日本語").own_file_count, 1);
    assert_eq!(dir("deep/a/b/c/d/e/f/g/h").logical_bytes, 5000);
    assert_eq!(dir("deep").dir_count, 8);
    assert_eq!(dir("emptydir").file_count, 0);

    // Kinds: by folder, by name, and NTFS's own files as System.
    let kinds = tree.root().kind_bytes;
    assert_eq!(
        dir("node_modules").kind_bytes[Kind::Developer as usize],
        73_728
    );
    assert_eq!(dir("Program Files").kind_bytes[Kind::Apps as usize], 90_112);
    assert_eq!(dir("big").kind_bytes[Kind::Videos as usize], 2_502_656);
    assert!(
        kinds[Kind::System as usize] >= 4_000_000,
        "$MFT, $LogFile and friends: {}",
        kinds[Kind::System as usize]
    );
    let bad_clusters = tree.files.iter().find(|f| f.name == "$BadClus");
    assert!(
        bad_clusters.is_none(),
        "the bad-cluster map spans the volume but owns none of it"
    );
}

#[test]
fn default_options_keep_only_large_files() {
    let tree = scan_volume(
        Cursor::new(image()),
        &MftOptions::new(ROOT),
        &NoClassify,
        &ScanControl::new(),
    )
    .unwrap();
    let names: Vec<String> = tree
        .files
        .iter()
        .map(|f| f.name.to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["movie.mp4", "$LogFile", "$MFT"]);
    assert_eq!(
        tree.root().alloc_bytes,
        expected().used,
        "what is kept never changes the totals"
    );
}

#[test]
fn progress_and_cancel() {
    let ctl = ScanControl::new();
    let tree = scan_volume(
        Cursor::new(image()),
        &MftOptions::new(ROOT),
        &NoClassify,
        &ctl,
    )
    .unwrap();
    assert!(
        ctl.files() >= tree.root().file_count - 2,
        "{} vs {}",
        ctl.files(),
        tree.root().file_count
    );
    assert!(ctl.dirs() >= tree.root().dir_count);

    let ctl = ScanControl::new();
    ctl.cancel();
    assert!(matches!(
        scan_volume(
            Cursor::new(image()),
            &MftOptions::new(ROOT),
            &NoClassify,
            &ctl
        ),
        Err(MftError::Cancelled)
    ));
}

#[test]
fn not_ntfs_is_reported_as_such() {
    assert!(is_ntfs(&mut Cursor::new(image())));
    let zeros = vec![0u8; 1 << 20];
    assert!(!is_ntfs(&mut Cursor::new(zeros.clone())));
    assert!(matches!(
        scan_volume(
            Cursor::new(zeros),
            &MftOptions::new(ROOT),
            &NoClassify,
            &ScanControl::new()
        ),
        Err(MftError::NotNtfs)
    ));
    let tiny = vec![0u8; 100];
    assert!(scan_volume(
        Cursor::new(tiny),
        &MftOptions::new(ROOT),
        &NoClassify,
        &ScanControl::new()
    )
    .is_err());
}

/// Damage must never crash the reader or make it read out of bounds. Flip
/// bytes all over the image, thousands of times, and scan each result.
#[test]
fn damaged_volumes_never_panic() {
    let clean = image();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    // Find where the file table starts, to aim half the damage at it.
    let mft_at = u64::from_le_bytes(clean[0x30..0x38].try_into().unwrap()) as usize * 4096;
    let (mut ok, mut failed) = (0, 0);
    for round in 0..400 {
        let mut img = clean.clone();
        for _ in 0..(1 + round % 40) {
            let at = match next() % 4 {
                0 => (next() % 512) as usize,                         // boot sector
                1 | 2 => mft_at + (next() % (1_600 * 1024)) as usize, // file records
                _ => (next() % img.len() as u64) as usize,            // anywhere
            };
            if at < img.len() {
                img[at] = next() as u8;
            }
        }
        match scan_volume(
            Cursor::new(img),
            &MftOptions::new(ROOT),
            &NoClassify,
            &ScanControl::new(),
        ) {
            Ok(tree) => {
                // Whatever was salvaged is still a consistent tree.
                for d in &tree.dirs[1..] {
                    assert!(d.parent.is_some());
                }
                ok += 1;
            }
            Err(_) => failed += 1,
        }
    }
    assert!(
        ok > 50 && failed > 5,
        "ok {ok}, failed {failed}: the damage should produce both outcomes"
    );
}

/// A larger image is too big to keep in the repository. Build one with
/// `tools/make-ntfs-fixture.sh <prefix> 200 60000` and point
/// `STRATA_NTFS_FIXTURE` at the prefix to run this.
#[test]
fn large_image_total_and_file_count_match() {
    let Ok(prefix) = std::env::var("STRATA_NTFS_FIXTURE") else {
        return;
    };
    let listing = std::fs::read_to_string(format!("{prefix}.expected.tsv")).unwrap();
    let used: u64 = listing
        .lines()
        .last()
        .unwrap()
        .split('\t')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let files = listing.lines().filter(|l| l.starts_with("f\t")).count() as u64;
    let dirs = listing.lines().filter(|l| l.starts_with("d\t")).count() as u64;

    let volume = std::fs::File::open(format!("{prefix}.img")).unwrap();
    let started = std::time::Instant::now();
    let tree = scan_volume(
        volume,
        &MftOptions::new(ROOT),
        &KindClassifier,
        &ScanControl::new(),
    )
    .unwrap();
    eprintln!(
        "{} files in {:?}",
        tree.root().file_count,
        started.elapsed()
    );
    assert_eq!(tree.root().alloc_bytes, used);
    // Everything listed, plus NTFS's own dozen or so files and its $Extend folder.
    assert!(
        (10..=16).contains(&(tree.root().file_count - files)),
        "{} vs {files}",
        tree.root().file_count
    );
    assert!(
        (1..=4).contains(&(tree.root().dir_count - dirs)),
        "{} vs {dirs}",
        tree.root().dir_count
    );
}
