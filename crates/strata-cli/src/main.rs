//! `strata scan <folder>`: run the scanner and print what it found.
//! `strata history <file>` and `strata growth <file>`: read saved scans.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use strata_classify::{attribute, Env, KindClassifier, Os, RuleSet};
use strata_core::{scan, volume_usage, Kind, ScanControl, ScanOptions, ScanTree};
use strata_store::{ScanMeta, Store};

const USAGE: &str = "\
Usage: strata scan <folder> [options]
       strata history <file>     List the scans saved in a history file
       strata growth <file>      What changed between the last two saved scans

Options for scan:
  --save <file>        Add the result to a history file (created if missing)
  --top <n>            How many folders and files to list (default 20)
  --all-filesystems    Also enter other volumes mounted below the folder
  --threads <n>        Worker threads (default: one per CPU core)
  --errors             List what could not be read
  --json               Print a one-line JSON summary instead of the report
  --quiet              No progress line while scanning";

struct Args {
    root: PathBuf,
    top: usize,
    all_filesystems: bool,
    threads: usize,
    errors: bool,
    json: bool,
    quiet: bool,
    save: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args_os().skip(2);
    let mut args = Args {
        root: PathBuf::new(),
        top: 20,
        all_filesystems: false,
        threads: 0,
        errors: false,
        json: false,
        quiet: false,
        save: None,
    };
    let mut root = None;
    while let Some(a) = it.next() {
        let mut number = |flag: &str| -> Result<usize, String> {
            it.next()
                .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| format!("{flag} needs a number"))
        };
        match a.to_str() {
            Some("--top") => args.top = number("--top")?,
            Some("--threads") => args.threads = number("--threads")?,
            Some("--save") => {
                args.save = Some(PathBuf::from(it.next().ok_or("--save needs a file name")?))
            }
            Some("--all-filesystems") => args.all_filesystems = true,
            Some("--errors") => args.errors = true,
            Some("--json") => args.json = true,
            Some("--quiet") => args.quiet = true,
            Some("--help") | Some("-h") => return Err(String::new()),
            Some(flag) if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ if root.is_none() => root = Some(PathBuf::from(a)),
            _ => return Err("only one folder can be scanned at a time".into()),
        }
    }
    args.root = root.ok_or("which folder should be scanned?")?;
    Ok(args)
}

fn main() -> ExitCode {
    let mut argv = std::env::args_os().skip(1);
    let command = argv.next().and_then(|c| c.into_string().ok());
    match (command.as_deref(), argv.next()) {
        (Some("scan"), _) => run_scan(),
        (Some("history"), Some(file)) => report(history(&PathBuf::from(file))),
        (Some("growth"), Some(file)) => report(growth(&PathBuf::from(file))),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn report(result: Result<(), strata_store::StoreError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("strata: {e}");
            ExitCode::FAILURE
        }
    }
}

fn history(file: &std::path::Path) -> Result<(), strata_store::StoreError> {
    let store = Store::open(file)?;
    for s in store.scans(None)? {
        println!(
            "#{:<4} {:>10}  {:>12} files  {}{}",
            s.id,
            human(s.alloc_bytes),
            group(s.file_count),
            s.root.display(),
            if s.has_tree { "" } else { "  [totals only]" }
        );
    }
    Ok(())
}

fn growth(file: &std::path::Path) -> Result<(), strata_store::StoreError> {
    let store = Store::open(file)?;
    let scans = store.scans(None)?;
    let Some(newest) = scans.first() else {
        println!("No scans saved yet.");
        return Ok(());
    };
    let Some(previous) = scans.iter().skip(1).find(|s| s.volume_id == newest.volume_id) else {
        println!("Only one scan of {} so far; scan again later to see changes.", newest.root.display());
        return Ok(());
    };
    println!(
        "{}: {} -> {} between scan #{} and #{}",
        newest.root.display(),
        human(previous.alloc_bytes),
        human(newest.alloc_bytes),
        previous.id,
        newest.id
    );
    for row in store.diff(previous.id, newest.id, 20)? {
        let sign = if row.delta() >= 0 { '+' } else { '-' };
        let name = if row.path.is_empty() { "(whole scan)" } else { &row.path };
        println!("{sign}{:>10}  {}", human(row.delta().unsigned_abs()), name);
    }
    Ok(())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn run_scan() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("strata: {msg}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let mut opts = ScanOptions::new(&args.root);
    opts.one_file_system = !args.all_filesystems;
    opts.threads = args.threads;
    let ctl = ScanControl::new();
    let show_progress = !args.quiet && !args.json;

    let started_at = now();
    let done = std::sync::atomic::AtomicBool::new(false);
    let result = std::thread::scope(|s| {
        let (ctl, done) = (&ctl, &done);
        if show_progress {
            s.spawn(move || {
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    eprint!(
                        "\r  {} files, {} on disk …   ",
                        group(ctl.files()),
                        human(ctl.bytes())
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
                eprint!("\r{:60}\r", "");
            });
        }
        let r = scan(&opts, &KindClassifier, ctl);
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        r
    });

    let tree = match result {
        Ok(t) => t,
        Err(e) => {
            eprintln!("strata: cannot scan {}: {e}", args.root.display());
            return ExitCode::FAILURE;
        }
    };

    if args.json {
        print_json(&args, &tree);
    } else {
        print_report(&args, &tree);
    }

    if let Some(file) = &args.save {
        // Until volumes get real identities (Day 11), the scanned path stands in.
        let meta = ScanMeta {
            volume_id: args.root.to_string_lossy().into_owned(),
            started_at,
            finished_at: now(),
        };
        match Store::open(file).and_then(|mut s| s.save(&tree, &meta)) {
            Ok(id) if !args.json => println!("\nSaved as scan #{id} in {}", file.display()),
            Ok(_) => {}
            Err(e) => {
                eprintln!("strata: could not save the scan: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn print_report(args: &Args, tree: &ScanTree) {
    let root = tree.root();
    let secs = tree.stats.elapsed_ms as f64 / 1000.0;
    let rate = if secs > 0.0 { root.file_count as f64 / secs } else { 0.0 };
    println!(
        "Scanned {} in {:.2} s: {} files, {} folders ({} files/s)",
        args.root.display(),
        secs,
        group(root.file_count),
        group(root.dir_count),
        group(rate as u64)
    );
    println!(
        "On disk: {}   (file lengths add up to {})",
        human(root.alloc_bytes),
        human(root.logical_bytes)
    );
    if root.cloud_only_bytes > 0 {
        println!("Cloud-only, not on this disk: {}", human(root.cloud_only_bytes));
    }
    if tree.stats.hardlink_duplicates > 0 {
        println!(
            "Hard links: {} extra links, {} not counted twice",
            group(tree.stats.hardlink_duplicates),
            human(tree.stats.hardlink_saved_bytes)
        );
    }
    let skipped: Vec<_> = tree
        .dirs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.flags.other_filesystem)
        .collect();
    if !skipped.is_empty() {
        println!("Other volumes not entered: {}", skipped.len());
    }
    if tree.stats.error_count > 0 {
        println!(
            "Not scanned: {} items could not be read{}",
            group(tree.stats.error_count),
            if args.errors { "" } else { " (--errors lists them)" }
        );
    }
    if tree.stats.cancelled {
        println!("The scan was cancelled; figures are partial.");
    }
    if let Ok(v) = volume_usage(&args.root) {
        let used = v.used_bytes();
        let share = if used > 0 { root.alloc_bytes as f64 / used as f64 * 100.0 } else { 0.0 };
        println!(
            "Volume: {} used of {}; this scan covers {:.1}% of the used space",
            human(used),
            human(v.total_bytes),
            share
        );
    }

    let mut kinds: Vec<(Kind, u64)> = Kind::ALL.iter().map(|&k| (k, root.kind_bytes[k as usize])).collect();
    kinds.sort_by_key(|&(_, bytes)| std::cmp::Reverse(bytes));
    let file_bytes: u64 = kinds.iter().map(|&(_, b)| b).sum();
    println!("\nBy kind");
    for (kind, bytes) in kinds.into_iter().filter(|&(_, b)| b > 0) {
        println!("{:>10}  {:>5.1}%  {}", human(bytes), percent(bytes, file_bytes), kind.name());
    }

    if let Some(env) = Env::current() {
        // Rules exist for macOS and Windows; on other systems this stays empty.
        let rules = RuleSet::builtin(Os::current());
        let apps = attribute(tree, &rules, &env).apps;
        if !apps.is_empty() {
            println!("\nBy app{:>31}", "app / data / cache");
            for app in apps.iter().take(args.top) {
                println!(
                    "{:>10}  {:<24} {} / {} / {}",
                    human(app.total_bytes()),
                    app.name,
                    human(app.app_bytes),
                    human(app.data_bytes),
                    human(app.cache_bytes)
                );
            }
        }
    }

    let mut children: Vec<u32> = root.children.clone();
    children.sort_by_key(|&c| std::cmp::Reverse(tree.dirs[c as usize].alloc_bytes));
    if !children.is_empty() {
        println!("\nLargest folders");
        for &c in children.iter().take(args.top) {
            let d = &tree.dirs[c as usize];
            let note = if d.flags.other_filesystem {
                "  [other volume]"
            } else if d.flags.unreadable {
                "  [not readable]"
            } else {
                ""
            };
            println!(
                "{:>10}  {:>5.1}%  {}{}",
                human(d.alloc_bytes),
                percent(d.alloc_bytes, root.alloc_bytes),
                d.name.to_string_lossy(),
                note
            );
        }
        if root.own_files_alloc > 0 {
            println!(
                "{:>10}  {:>5.1}%  (files directly in this folder)",
                human(root.own_files_alloc),
                percent(root.own_files_alloc, root.alloc_bytes)
            );
        }
    }

    if !tree.files.is_empty() {
        println!("\nLargest files");
        for f in tree.files.iter().take(args.top) {
            println!("{:>10}  {}", human(f.alloc_bytes), tree.file_path(f).display());
        }
    }

    if args.errors && !tree.errors.is_empty() {
        println!("\nCould not read");
        for e in &tree.errors {
            println!("  {}: {}", e.path.display(), e.message);
        }
        let hidden = tree.stats.error_count.saturating_sub(tree.errors.len() as u64);
        if hidden > 0 {
            println!("  … and {} more", group(hidden));
        }
    }
}

fn print_json(args: &Args, tree: &ScanTree) {
    let root = tree.root();
    let volume_used = volume_usage(&args.root)
        .map(|v| v.used_bytes().to_string())
        .unwrap_or_else(|_| "null".into());
    println!(
        "{{\"alloc_bytes\":{},\"logical_bytes\":{},\"files\":{},\"dirs\":{},\"cloud_only_bytes\":{},\
         \"hardlink_saved_bytes\":{},\"errors\":{},\"cancelled\":{},\"elapsed_ms\":{},\"volume_used_bytes\":{}}}",
        root.alloc_bytes,
        root.logical_bytes,
        root.file_count,
        root.dir_count,
        root.cloud_only_bytes,
        tree.stats.hardlink_saved_bytes,
        tree.stats.error_count,
        tree.stats.cancelled,
        tree.stats.elapsed_ms,
        volume_used
    );
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}

/// Decimal units (1 GB = 1,000,000,000 bytes), as macOS and drive makers use.
fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 1234567 -> "1,234,567"
fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
