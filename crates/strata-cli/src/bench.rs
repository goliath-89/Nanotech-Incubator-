//! `strata make-tree` and `strata bench`: measure the scanner on a known tree.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use strata_classify::KindClassifier;
use strata_core::{scan, ScanControl, ScanOptions};

/// Create a folder tree with `files` files, shaped roughly like a real
/// drive: about ten files per folder, four to six levels deep, mostly small
/// files with a few large ones. The same arguments always give the same tree.
pub fn make_tree(root: &Path, files: u64) -> std::io::Result<()> {
    if root.exists() && fs::read_dir(root)?.next().is_some() {
        return Err(std::io::Error::other("the folder exists and is not empty"));
    }
    fs::create_dir_all(root)?;
    let workers = std::thread::available_parallelism().map(|n| n.get() as u64).unwrap_or(4).min(16);
    let started = Instant::now();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| scope.spawn(move || make_share(root, files, w, workers)))
            .collect();
        handles.into_iter().try_for_each(|h| h.join().expect("worker panicked"))
    })?;
    println!("Created {files} files under {} in {:.1} s", root.display(), started.elapsed().as_secs_f64());
    Ok(())
}

fn make_share(root: &Path, files: u64, worker: u64, workers: u64) -> std::io::Result<()> {
    const EXTENSIONS: [&str; 12] = ["jpg", "mp4", "txt", "pdf", "js", "json", "png", "dll", "log", "bin", "mov", "db"];
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
            0..=59 => {}                                  // empty
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
        let tree = scan(&ScanOptions::new(root), &KindClassifier, &ScanControl::new())?;
        let secs = tree.stats.elapsed_ms as f64 / 1000.0;
        summary = (tree.root().file_count, tree.root().dir_count, tree.root().alloc_bytes);
        println!("run {run}: {secs:.2} s");
        times.push(secs);
    }
    times.sort_by(f64::total_cmp);
    let best = times[0];
    let median = times[times.len() / 2];
    let (files, dirs, bytes) = summary;
    println!("{files} files, {dirs} folders, {:.1} GB on disk", bytes as f64 / 1e9);
    println!("best {best:.2} s, median {median:.2} s, {:.0} files/s at best", files as f64 / best.max(0.001));
    match peak_memory_mb() {
        Some(mb) => println!("peak memory {mb:.0} MB"),
        None => println!("peak memory: not measured on this system"),
    }
    // One line for pasting back and for scripts.
    println!(
        "RESULT os={} files={files} dirs={dirs} best_s={best:.2} median_s={median:.2} peak_mb={}",
        std::env::consts::OS,
        peak_memory_mb().map(|m| format!("{m:.0}")).unwrap_or_else(|| "na".into())
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

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn peak_memory_mb() -> Option<f64> {
    None
}
