use crate::platform::{self, RootCtx};
use crate::tree::{
    Classify, DirFlags, DirNode, FileNode, Kind, ScanError, ScanErrorKind, ScanStats, ScanTree,
    KIND_COUNT,
};
use rayon::prelude::*;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::Instant;

/// How many unreadable paths are kept with full detail. The count of all of
/// them is always exact (`ScanStats::error_count`).
const MAX_RECORDED_ERRORS: usize = 10_000;
/// Deep trees recurse deeply, and rayon workers run other folders' jobs while
/// they wait, so give the workers generous stacks.
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub root: PathBuf,
    /// Stay on the root's file system and do not enter other mounted volumes.
    pub one_file_system: bool,
    /// Files at least this large are kept individually in `ScanTree::files`.
    pub min_file_bytes: u64,
    /// Upper bound on `ScanTree::files`; the largest are kept.
    pub max_files: usize,
    /// Worker threads; 0 picks one per CPU core.
    pub threads: usize,
    /// Folders that must not be entered (exact paths).
    pub excludes: Vec<PathBuf>,
}

impl ScanOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        ScanOptions {
            root: root.into(),
            one_file_system: true,
            min_file_bytes: 1024 * 1024,
            max_files: 50_000,
            threads: 0,
            excludes: Vec::new(),
        }
    }
}

/// Shared between the scanning threads and whoever watches the scan: live
/// counters for a progress display, and the cancel switch.
#[derive(Debug, Default)]
pub struct ScanControl {
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    cancel: AtomicBool,
}

impl ScanControl {
    pub fn new() -> Self {
        Self::default()
    }
    /// Ask the scan to stop. It returns what it has counted so far.
    pub fn cancel(&self) {
        self.cancel.store(true, Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Relaxed)
    }
    /// Files seen so far.
    pub fn files(&self) -> u64 {
        self.files.load(Relaxed)
    }
    /// Folders listed so far.
    pub fn dirs(&self) -> u64 {
        self.dirs.load(Relaxed)
    }
    /// Bytes on disk counted so far. Hard-linked files are added at the end,
    /// so this can finish slightly below the final total.
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Relaxed)
    }
    /// Entries that could not be read so far.
    pub fn errors(&self) -> u64 {
        self.errors.load(Relaxed)
    }
}

struct RawFile {
    name: OsString,
    alloc: u64,
    logical: u64,
    kind: Kind,
}

struct RawLink {
    id: (u64, u64),
    file: RawFile,
}

/// One folder as the walk found it: only what sits directly inside it.
struct RawDir {
    name: OsString,
    dir_alloc: u64,
    files_alloc: u64,
    files_logical: u64,
    file_count: u64,
    kind_bytes: [u64; KIND_COUNT],
    cloud_only: u64,
    big: Vec<RawFile>,
    links: Vec<RawLink>,
    children: Vec<RawDir>,
    flags: DirFlags,
}

impl RawDir {
    fn new(name: OsString, dir_alloc: u64) -> Self {
        RawDir {
            name,
            dir_alloc,
            files_alloc: 0,
            files_logical: 0,
            file_count: 0,
            kind_bytes: [0; KIND_COUNT],
            cloud_only: 0,
            big: Vec::new(),
            links: Vec::new(),
            children: Vec::new(),
            flags: DirFlags::default(),
        }
    }
}

struct Ctx<'a> {
    opts: &'a ScanOptions,
    classify: &'a dyn Classify,
    ctl: &'a ScanControl,
    root: RootCtx,
    errors: Mutex<Vec<ScanError>>,
}

impl Ctx<'_> {
    fn record_error(&self, path: PathBuf, err: &io::Error) {
        self.ctl.errors.fetch_add(1, Relaxed);
        let mut errors = self.errors.lock().unwrap_or_else(|e| e.into_inner());
        if errors.len() < MAX_RECORDED_ERRORS {
            let kind = match err.kind() {
                io::ErrorKind::PermissionDenied => ScanErrorKind::PermissionDenied,
                io::ErrorKind::NotFound => ScanErrorKind::NotFound,
                _ => ScanErrorKind::Other,
            };
            errors.push(ScanError {
                path,
                kind,
                message: err.to_string(),
            });
        }
    }
}

/// Scan `opts.root` and everything below it.
///
/// Fails only when the root itself cannot be read. Problems further down are
/// recorded in the result and the scan carries on.
pub fn scan(opts: &ScanOptions, classify: &dyn Classify, ctl: &ScanControl) -> io::Result<ScanTree> {
    let started = Instant::now();
    let root = platform::root_ctx(&opts.root)?;
    // Listing the root must work, or there is nothing to show.
    std::fs::read_dir(&opts.root)?;

    let ctx = Ctx {
        opts,
        classify,
        ctl,
        root,
        errors: Mutex::new(Vec::new()),
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads)
        .stack_size(WORKER_STACK_BYTES)
        .thread_name(|i| format!("strata-scan-{i}"))
        .build()
        .map_err(io::Error::other)?;

    let mut tree = pool.install(|| {
        let raw = scan_dir(
            &ctx,
            opts.root.clone(),
            opts.root.clone().into_os_string(),
            root.alloc,
        );
        build_tree(raw, opts)
    });

    tree.errors = ctx.errors.into_inner().unwrap_or_else(|e| e.into_inner());
    tree.stats.error_count = ctl.errors();
    tree.stats.cancelled = ctl.is_cancelled();
    tree.stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(tree)
}

fn scan_dir(ctx: &Ctx, path: PathBuf, name: OsString, dir_alloc: u64) -> RawDir {
    let mut dir = RawDir::new(name, dir_alloc);
    if ctx.ctl.is_cancelled() {
        dir.flags.cancelled = true;
        return dir;
    }
    let entries = match std::fs::read_dir(&path) {
        Ok(e) => e,
        Err(e) => {
            ctx.record_error(path, &e);
            dir.flags.unreadable = true;
            return dir;
        }
    };
    ctx.ctl.dirs.fetch_add(1, Relaxed);

    // (path, name, the folder's own bytes, flags decided here)
    let mut subdirs: Vec<(PathBuf, OsString, u64, DirFlags)> = Vec::new();
    let mut files_here = 0u64;
    let mut bytes_here = 0u64;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                ctx.record_error(path.clone(), &e);
                continue;
            }
        };
        let info = match platform::entry_info(&entry, &ctx.root) {
            Ok(i) => i,
            Err(e) => {
                ctx.record_error(entry.path(), &e);
                continue;
            }
        };
        let entry_name = entry.file_name();

        if info.is_dir {
            let sub_path = path.join(&entry_name);
            let mut flags = DirFlags::default();
            if ctx.opts.one_file_system && info.dev != ctx.root.dev {
                flags.other_filesystem = true;
            } else if ctx.opts.excludes.contains(&sub_path) {
                flags.excluded = true;
            }
            // A folder on another file system takes no space on this one.
            let own = if flags.other_filesystem { 0 } else { info.alloc };
            bytes_here += own;
            subdirs.push((sub_path, entry_name, own, flags));
            continue;
        }

        // Files, symbolic links, sockets, devices: anything that is not a folder.
        files_here += 1;
        dir.file_count += 1;
        let file = RawFile {
            kind: ctx.classify.file_kind(&path, &entry_name),
            name: entry_name,
            alloc: info.alloc,
            logical: info.logical,
        };
        if info.cloud_only {
            dir.cloud_only += info.logical;
        }
        if let Some(id) = info.link_id {
            // Counted once, after the walk, when all its links are known.
            dir.links.push(RawLink { id, file });
            continue;
        }
        bytes_here += file.alloc;
        dir.files_alloc += file.alloc;
        dir.files_logical += file.logical;
        dir.kind_bytes[file.kind as usize] += file.alloc;
        if file.alloc >= ctx.opts.min_file_bytes {
            dir.big.push(file);
        }
    }

    ctx.ctl.files.fetch_add(files_here, Relaxed);
    ctx.ctl.bytes.fetch_add(bytes_here, Relaxed);

    dir.children = subdirs
        .into_par_iter()
        .map(|(sub_path, sub_name, own, flags)| {
            if flags.not_scanned() {
                let mut d = RawDir::new(sub_name, own);
                d.flags = flags;
                d
            } else {
                scan_dir(ctx, sub_path, sub_name, own)
            }
        })
        .collect();
    dir
}

struct PendingLink {
    id: (u64, u64),
    dir: u32,
    file: RawFile,
}

fn build_tree(raw: RawDir, opts: &ScanOptions) -> ScanTree {
    let mut dirs: Vec<DirNode> = Vec::new();
    let mut files: Vec<FileNode> = Vec::new();
    let mut links: Vec<PendingLink> = Vec::new();
    flatten(raw, None, &mut dirs, &mut files, &mut links);

    // Hard links: each file is counted once, in the first folder (in tree
    // order, then by name) that holds a link to it. Sorting makes the choice
    // the same on every run, whatever order the threads finished in.
    let mut stats = ScanStats::default();
    links.sort_by(|a, b| {
        (a.id, a.dir)
            .cmp(&(b.id, b.dir))
            .then_with(|| a.file.name.cmp(&b.file.name))
    });
    let mut previous: Option<(u64, u64)> = None;
    for link in links {
        if previous == Some(link.id) {
            stats.hardlink_duplicates += 1;
            stats.hardlink_saved_bytes += link.file.alloc;
            continue;
        }
        previous = Some(link.id);
        let d = &mut dirs[link.dir as usize];
        d.alloc_bytes += link.file.alloc;
        d.logical_bytes += link.file.logical;
        d.own_files_alloc += link.file.alloc;
        d.kind_bytes[link.file.kind as usize] += link.file.alloc;
        if link.file.alloc >= opts.min_file_bytes {
            files.push(FileNode {
                dir: link.dir,
                name: link.file.name,
                alloc_bytes: link.file.alloc,
                logical_bytes: link.file.logical,
                kind: link.file.kind,
            });
        }
    }

    // Roll totals up. Children always have a higher index than their parent,
    // so walking backwards finishes every folder before it is added upwards.
    for i in (1..dirs.len()).rev() {
        let (head, tail) = dirs.split_at_mut(i);
        let child = &tail[0];
        let parent = &mut head[child.parent.expect("only the root has no parent") as usize];
        parent.alloc_bytes += child.alloc_bytes;
        parent.logical_bytes += child.logical_bytes;
        parent.file_count += child.file_count;
        parent.dir_count += child.dir_count + 1;
        parent.cloud_only_bytes += child.cloud_only_bytes;
        for k in 0..KIND_COUNT {
            parent.kind_bytes[k] += child.kind_bytes[k];
        }
    }

    files.sort_by(|a, b| {
        b.alloc_bytes
            .cmp(&a.alloc_bytes)
            .then_with(|| a.dir.cmp(&b.dir))
            .then_with(|| a.name.cmp(&b.name))
    });
    files.truncate(opts.max_files);
    files.shrink_to_fit();

    ScanTree {
        dirs,
        files,
        errors: Vec::new(),
        stats,
    }
}

fn flatten(
    mut raw: RawDir,
    parent: Option<u32>,
    dirs: &mut Vec<DirNode>,
    files: &mut Vec<FileNode>,
    links: &mut Vec<PendingLink>,
) -> u32 {
    let id = u32::try_from(dirs.len()).expect("more than 4 billion folders");
    dirs.push(DirNode {
        parent,
        name: raw.name,
        children: Vec::with_capacity(raw.children.len()),
        alloc_bytes: raw.dir_alloc + raw.files_alloc,
        logical_bytes: raw.files_logical,
        file_count: raw.file_count,
        dir_count: 0,
        kind_bytes: raw.kind_bytes,
        cloud_only_bytes: raw.cloud_only,
        own_files_alloc: raw.files_alloc,
        own_file_count: raw.file_count,
        flags: raw.flags,
    });
    files.extend(raw.big.into_iter().map(|f| FileNode {
        dir: id,
        name: f.name,
        alloc_bytes: f.alloc,
        logical_bytes: f.logical,
        kind: f.kind,
    }));
    links.extend(raw.links.into_iter().map(|l| PendingLink {
        id: l.id,
        dir: id,
        file: l.file,
    }));
    raw.children.sort_by(|a, b| a.name.cmp(&b.name));
    for child in raw.children {
        let cid = flatten(child, Some(id), dirs, files, links);
        dirs[id as usize].children.push(cid);
    }
    id
}
