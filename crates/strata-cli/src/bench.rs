//! `strata make-tree` and `strata bench`: measure the scanner on a known tree.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use strata_classify::KindClassifier;
use strata_core::raw::{build_tree, RawDir, RawFile};
use strata_core::{scan, Kind, ScanControl, ScanOptions, ScanTree};

/// Create a folder tree with `files` files, shaped roughly like a real
/// drive: about ten files per folder, four to six levels deep, mostly small
/// files with a few large ones. The same arguments always give the same tree.
pub fn make_tree(root: &Path, files: u64) -> std::io::Result<()> {
    if root.exists() && fs::read_dir(root)?.next().is_some() {
        return Err(std::io::Error::other("the folder exists and is not empty"));
    }
    fs::create_dir_all(root)?;
    let workers = std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(4)
        .min(16);
    let started = Instant::now();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| scope.spawn(move || make_share(root, files, w, workers)))
            .collect();
        handles
            .into_iter()
            .try_for_each(|h| h.join().expect("worker panicked"))
    })?;
    println!(
        "Created {files} files under {} in {:.1} s",
        root.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn make_share(root: &Path, files: u64, worker: u64, workers: u64) -> std::io::Result<()> {
    const EXTENSIONS: [&str; 12] = [
        "jpg", "mp4", "txt", "pdf", "js", "json", "png", "dll", "log", "bin", "mov", "db",
    ];
    let page = [0x5Au8; 4096];
    let mut current_dir = PathBuf::new();
    let mut i = worker;
    while i < files {
        // Ten files per folder; folders fan out 16 x 32 x 40 x ...
        let folder = i / 10;
        let dir = root
            .join(format!("top{:02}", folder % 16))
            .join(format!("mid{:02}", (folder / 16) % 32))
            .join(format!("sub{:02}", (folder / 512) % 40))
            .join(format!("leaf{}", folder / 20_480));
        if dir != current_dir {
            fs::create_dir_all(&dir)?;
            current_dir = dir;
        }
        let mix = i.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
        let path = current_dir.join(format!("f{i}.{}", EXTENSIONS[(mix % 12) as usize]));
        let mut file = fs::File::create(&path)?;
        match mix % 100 {
            0..=59 => {}                                                // empty
            60..=94 => file.write_all(&page[..(mix % 4096) as usize])?, // under one block
            95..=98 => file.write_all(&page)?,
            _ => file.set_len(2 * 1024 * 1024 + mix % 65_536)?, // large, sparse where supported
        }
        i += workers;
    }
    Ok(())
}

/// Scan `root` `runs` times and report the timings.
pub fn bench(root: &Path, runs: usize) -> std::io::Result<()> {
    let mut times: Vec<f64> = Vec::new();
    let mut summary = (0u64, 0u64, 0u64);
    for run in 1..=runs.max(1) {
        let tree = scan(
            &ScanOptions::new(root),
            &KindClassifier,
            &ScanControl::new(),
        )?;
        let secs = tree.stats.elapsed_ms as f64 / 1000.0;
        summary = (
            tree.root().file_count,
            tree.root().dir_count,
            tree.root().alloc_bytes,
        );
        println!("run {run}: {secs:.2} s");
        times.push(secs);
    }
    times.sort_by(f64::total_cmp);
    let best = times[0];
    let median = times[times.len() / 2];
    let (files, dirs, bytes) = summary;
    println!(
        "{files} files, {dirs} folders, {:.1} GB on disk",
        bytes as f64 / 1e9
    );
    println!(
        "best {best:.2} s, median {median:.2} s, {:.0} files/s at best",
        files as f64 / best.max(0.001)
    );
    match peak_memory_mb() {
        Some(mb) => println!("peak memory {mb:.0} MB"),
        None => println!("peak memory: not measured on this system"),
    }
    // One line for pasting back and for scripts.
    println!(
        "RESULT os={} files={files} dirs={dirs} best_s={best:.2} median_s={median:.2} peak_mb={}",
        std::env::consts::OS,
        peak_memory_mb()
            .map(|m| format!("{m:.0}"))
            .unwrap_or_else(|| "na".into())
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn peak_memory_mb() -> Option<f64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

#[cfg(target_os = "macos")]
fn peak_memory_mb() -> Option<f64> {
    // SAFETY: `usage` is a valid, writable rusage struct that the call fills in.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return None;
        }
        usage
    };
    Some(usage.ru_maxrss as f64 / (1024.0 * 1024.0)) // bytes on macOS
}

#[cfg(windows)]
fn peak_memory_mb() -> Option<f64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing;
    // `counters` is a zeroed, writable struct and `size` is its exact size.
    let counters = unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = size;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) == 0 {
            return None;
        }
        counters
    };
    Some(counters.PeakWorkingSetSize as f64 / (1024.0 * 1024.0))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn peak_memory_mb() -> Option<f64> {
    None
}

/// A scan of `files` files with the same shape as [`make_tree`], built in
/// memory (no disk needed). `grow` changes about 1% of the folders, for a
/// second scan to compare with.
fn synthetic_tree(files: u64, grow: bool) -> ScanTree {
    const KINDS: [Kind; 8] = [
        Kind::Photos,
        Kind::Videos,
        Kind::Documents,
        Kind::Developer,
        Kind::Apps,
        Kind::System,
        Kind::Caches,
        Kind::Other,
    ];
    let folders = files.div_ceil(10);
    let mut root = RawDir::new(OsString::from("/bench"), 4096);
    let mut tops: Vec<RawDir> = (0..16)
        .map(|t| RawDir::new(format!("top{t:02}").into(), 4096))
        .collect();
    // Build leaves grouped by their path so each folder is made once.
    let mut by_path: std::collections::BTreeMap<(u64, u64, u64, u64), RawDir> = Default::default();
    for folder in 0..folders {
        let key = (
            folder % 16,
            (folder / 16) % 32,
            (folder / 512) % 40,
            folder / 20_480,
        );
        let leaf = by_path
            .entry(key)
            .or_insert_with(|| RawDir::new(format!("leaf{}", key.3).into(), 4096));
        for i in folder * 10..((folder + 1) * 10).min(files) {
            let mix = i.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
            let mut alloc = match mix % 100 {
                0..=59 => 0,
                60..=98 => 4096,
                _ => 2 * 1024 * 1024 + (mix % 65_536) * 4096,
            };
            if grow && folder % 97 == 0 {
                alloc += 3 * 1024 * 1024;
            }
            leaf.add_file(
                RawFile {
                    name: format!("f{i}").into(),
                    alloc,
                    logical: alloc,
                    kind: KINDS[(mix % 8) as usize],
                },
                1024 * 1024,
            );
        }
    }
    let mut mids: std::collections::BTreeMap<(u64, u64), RawDir> = Default::default();
    let mut subs: std::collections::BTreeMap<(u64, u64, u64), RawDir> = Default::default();
    for ((t, m, sb, _), leaf) in by_path {
        subs.entry((t, m, sb))
            .or_insert_with(|| RawDir::new(format!("sub{sb:02}").into(), 4096))
            .children
            .push(leaf);
    }
    for ((t, m, _), sub) in subs {
        mids.entry((t, m))
            .or_insert_with(|| RawDir::new(format!("mid{m:02}").into(), 4096))
            .children
            .push(sub);
    }
    for ((t, _), mid) in mids {
        tops[t as usize].children.push(mid);
    }
    root.children = tops;
    build_tree(root, 1024 * 1024, 50_000)
}

/// Time what the app does with a large scan: save it, open the top folder,
/// draw the ring, list the largest files, compare with an earlier scan,
/// slim old scans down, leave a folder out.
pub fn bench_app(files: u64, db: &Path) -> std::io::Result<()> {
    use strata_store::{ScanMeta, Store};
    let err = |e: strata_store::StoreError| std::io::Error::other(e.to_string());
    if db.exists() {
        return Err(std::io::Error::other(
            "the history file exists; give a new one",
        ));
    }
    let mut lines: Vec<String> = Vec::new();
    let mut step =
        |name: &str, f: &mut dyn FnMut() -> std::io::Result<String>| -> std::io::Result<()> {
            let t = Instant::now();
            let note = f()?;
            let line = format!("{name:<34} {:>8.2} s  {note}", t.elapsed().as_secs_f64());
            println!("{line}");
            lines.push(line);
            Ok(())
        };
    let mut before = None;
    step("build earlier scan (in memory)", &mut || {
        before = Some(synthetic_tree(files, false));
        let t = before.as_ref().unwrap();
        Ok(format!("{} folders", t.dirs.len()))
    })?;
    let mut tree = None;
    step("build scan (in memory)", &mut || {
        tree = Some(synthetic_tree(files, true));
        Ok(String::new())
    })?;
    let (before, tree) = (before.unwrap(), tree.unwrap());
    let mut store = Store::open(db).map_err(err)?;
    let meta = |t: i64| ScanMeta {
        volume_id: "bench".into(),
        started_at: t,
        finished_at: t,
    };
    let mut ids = (0, 0);
    step("save earlier scan", &mut || {
        ids.0 = strata_pipeline::save_everything(&mut store, &before, &meta(1)).map_err(err)?;
        Ok(String::new())
    })?;
    step("save scan", &mut || {
        ids.1 = strata_pipeline::save_everything(&mut store, &tree, &meta(2)).map_err(err)?;
        Ok(format!(
            "history file {:.0} MB",
            fs::metadata(db).map(|m| m.len()).unwrap_or(0) as f64 / 1e6
        ))
    })?;
    step("open the top folder", &mut || {
        let l = store.children(ids.1, 0).map_err(err)?;
        Ok(format!("{} folders listed", l.children.len()))
    })?;
    step("open a deep folder", &mut || {
        let deep = tree.dirs.len() as u32 - 1;
        store.children(ids.1, deep).map_err(err)?;
        Ok(String::new())
    })?;
    for attempt in [
        "ring for the top folder",
        "ring again (warm)",
        "ring for a top-level folder",
    ] {
        let dir = if attempt.contains("top-level") {
            tree.root().children[0]
        } else {
            0
        };
        step(attempt, &mut || {
            strata_pipeline::ring::ring_tree(&store, ids.1, dir, 3, 0.004).map_err(err)?;
            Ok(String::new())
        })?;
    }
    step("largest files", &mut || {
        let f = store.largest_files(ids.1, None, 100).map_err(err)?;
        Ok(format!("{} files", f.len()))
    })?;
    step("compare with the earlier scan", &mut || {
        let d = store.diff(ids.0, ids.1, 60).map_err(err)?;
        Ok(format!("{} changes", d.len()))
    })?;
    step("slim down older scans", &mut || {
        let n = store.prune("bench", 1).map_err(err)?;
        Ok(format!("{n} slimmed"))
    })?;
    let mut t2 = tree.clone();
    step("leave out one top folder", &mut || {
        strata_pipeline::exclude::apply(&mut t2, &[std::path::PathBuf::from("/bench/top03")]);
        Ok(format!("{} folders left", t2.dirs.len()))
    })?;
    match peak_memory_mb() {
        Some(mb) => println!("peak memory {mb:.0} MB"),
        None => println!("peak memory: not measured on this system"),
    }
    Ok(())
}
