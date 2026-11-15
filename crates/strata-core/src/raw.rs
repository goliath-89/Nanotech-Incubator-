//! The step between "what is in each folder" and a finished [`ScanTree`].
//!
//! The directory walk in [`crate::scan`] uses it, and so can any other way
//! of listing a volume (the NTFS file-table reader, for one), so every
//! scanner applies the same counting rules.

use crate::tree::{DirFlags, DirNode, FileNode, Kind, ScanStats, ScanTree, KIND_COUNT};
use std::ffi::OsString;

/// A file as a scanner found it.
pub struct RawFile {
    pub name: OsString,
    pub alloc: u64,
    pub logical: u64,
    pub kind: Kind,
}

/// One name of a file that has several (hard links). All names of the same
/// file carry the same `id`; the file's bytes are counted once.
pub struct RawLink {
    pub id: (u64, u64),
    pub file: RawFile,
}

/// One folder as a scanner found it: only what sits directly inside it.
/// Fill the fields, then hand the root to [`build_tree`].
pub struct RawDir {
    pub name: OsString,
    pub dir_alloc: u64,
    pub files_alloc: u64,
    pub files_logical: u64,
    pub file_count: u64,
    pub kind_bytes: [u64; KIND_COUNT],
    pub cloud_only: u64,
    pub big: Vec<RawFile>,
    pub links: Vec<RawLink>,
    pub children: Vec<RawDir>,
    pub flags: DirFlags,
}

impl RawDir {
    /// Count one file that has a single name. `min_file_bytes` is the
    /// threshold above which files are kept individually.
    pub fn add_file(&mut self, file: RawFile, min_file_bytes: u64) {
        self.file_count += 1;
        self.files_alloc += file.alloc;
        self.files_logical += file.logical;
        self.kind_bytes[file.kind as usize] += file.alloc;
        if file.alloc >= min_file_bytes {
            self.big.push(file);
        }
    }

    /// Count one name of a file that has several.
    pub fn add_link(&mut self, id: (u64, u64), file: RawFile) {
        self.file_count += 1;
        self.links.push(RawLink { id, file });
    }

    /// An empty folder. `dir_alloc` is the space the folder itself takes.
    pub fn new(name: OsString, dir_alloc: u64) -> Self {
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

struct PendingLink {
    id: (u64, u64),
    dir: u32,
    file: RawFile,
}

/// Turn what a scanner found into a [`ScanTree`]: number the folders,
/// settle which folder each hard-linked file is counted in, roll totals up
/// and keep the largest files. Recursion depth equals the tree's depth, so
/// call it on a thread with a generous stack for very deep trees.
pub fn build_tree(raw: RawDir, min_file_bytes: u64, max_files: usize) -> ScanTree {
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
        if link.file.alloc >= min_file_bytes {
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
    files.truncate(max_files);
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
