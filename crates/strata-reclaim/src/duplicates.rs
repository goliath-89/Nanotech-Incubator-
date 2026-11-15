use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use strata_core::{Kind, ScanTree};
use strata_safety::Policy;

/// How much of each end of a file the quick comparison reads.
const SAMPLE: u64 = 64 * 1024;
const READ_BUFFER: usize = 1024 * 1024;

/// Progress and cancel switch for a duplicate search.
#[derive(Debug, Default)]
pub struct DuplicateControl {
    bytes_read: AtomicU64,
    cancel: AtomicBool,
}

impl DuplicateControl {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Relaxed)
    }
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read.load(Relaxed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateFile {
    pub path: PathBuf,
    /// False for a copy the safety policy protects (inside an app or a
    /// photo library, say). It proves the others are duplicates but can
    /// never be the one removed.
    pub removable: bool,
}

/// Files with identical contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateGroup {
    /// Bytes on disk of one copy.
    pub alloc_bytes: u64,
    /// The copies. The first is the one to keep: a protected copy if there
    /// is one, otherwise the one with the shortest path.
    pub files: Vec<DuplicateFile>,
}

impl DuplicateGroup {
    /// Bytes freed by removing every removable copy but one in total.
    pub fn reclaimable_bytes(&self) -> u64 {
        let removable = self.files.iter().filter(|f| f.removable).count() as u64;
        let spare = removable.min(self.files.len() as u64 - 1);
        self.alloc_bytes * spare
    }
}

struct Candidate {
    path: PathBuf,
    len: u64,
    alloc: u64,
}

/// Find identical files among the large personal files a scan kept
/// (videos, photos, music, documents and unclassified files).
///
/// Files are grouped by length, then by a hash of their first and last
/// 64 KiB, then by a hash of everything. Only the last step proves equality,
/// and only files that survive the cheaper steps pay for it. A file that
/// changed since the scan, or cannot be read, is left out.
pub fn find_duplicates(
    tree: &ScanTree,
    policy: &Policy,
    ctl: &DuplicateControl,
) -> Vec<DuplicateGroup> {
    let mut by_len: HashMap<u64, Vec<Candidate>> = HashMap::new();
    for f in &tree.files {
        // Only the user's own material. Identical files inside apps, games,
        // developer tools, caches and the system are there on purpose, and
        // removing one "spare" would break whatever installed it.
        let personal = matches!(
            f.kind,
            Kind::Videos | Kind::Photos | Kind::Music | Kind::Documents | Kind::Other
        );
        if f.logical_bytes == 0 || !personal {
            continue;
        }
        by_len.entry(f.logical_bytes).or_default().push(Candidate {
            path: tree.file_path(f),
            len: f.logical_bytes,
            alloc: f.alloc_bytes,
        });
    }
    let same_length: Vec<Vec<Candidate>> = by_len.into_values().filter(|g| g.len() > 1).collect();

    let same_sample = regroup(same_length, ctl, |c| hash_sample(&c.path, c.len, ctl));
    let identical = regroup(same_sample, ctl, |c| {
        // The sample already covered all of a small file.
        if c.len <= 2 * SAMPLE {
            hash_sample(&c.path, c.len, ctl)
        } else {
            hash_all(&c.path, c.len, ctl)
        }
    });
    if ctl.is_cancelled() {
        return Vec::new(); // a half-finished comparison proves nothing
    }

    let mut groups: Vec<DuplicateGroup> = identical
        .into_iter()
        .filter_map(|mut members| {
            drop_same_file_twice(&mut members);
            if members.len() < 2 {
                return None;
            }
            let alloc_bytes = members.iter().map(|c| c.alloc).max().unwrap_or(0);
            let mut files: Vec<DuplicateFile> = members
                .into_iter()
                .map(|c| DuplicateFile {
                    removable: policy.check(&c.path).is_ok(),
                    path: c.path,
                })
                .collect();
            files.sort_by(|a, b| {
                (a.removable, a.path.components().count(), &a.path).cmp(&(
                    b.removable,
                    b.path.components().count(),
                    &b.path,
                ))
            });
            let group = DuplicateGroup { alloc_bytes, files };
            (group.reclaimable_bytes() > 0).then_some(group)
        })
        .collect();
    groups.sort_by(|a, b| {
        b.reclaimable_bytes()
            .cmp(&a.reclaimable_bytes())
            .then_with(|| a.files[0].path.cmp(&b.files[0].path))
    });
    groups
}

/// Split each group by `key`, dropping files whose key cannot be computed
/// and groups that end up with a single member.
fn regroup<F>(groups: Vec<Vec<Candidate>>, ctl: &DuplicateControl, key: F) -> Vec<Vec<Candidate>>
where
    F: Fn(&Candidate) -> io::Result<[u8; 32]> + Sync,
{
    groups
        .into_par_iter()
        .flat_map_iter(|group| {
            let mut split: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            for candidate in group {
                if ctl.is_cancelled() {
                    break;
                }
                if let Ok(k) = key(&candidate) {
                    split.entry(k).or_default().push(candidate);
                }
            }
            split.into_values().filter(|g| g.len() > 1)
        })
        .collect()
}

fn open_unchanged(path: &Path, expected_len: u64) -> io::Result<File> {
    // A link could point anywhere; the scan listed the link, not its target.
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::other("is a link"));
    }
    let file = File::open(path)?;
    if file.metadata()?.len() != expected_len {
        return Err(io::Error::other("changed since the scan"));
    }
    Ok(file)
}

fn hash_sample(path: &Path, len: u64, ctl: &DuplicateControl) -> io::Result<[u8; 32]> {
    let mut file = open_unchanged(path, len)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; SAMPLE as usize];
    let head = len.min(SAMPLE) as usize;
    file.read_exact(&mut buffer[..head])?;
    hasher.update(&buffer[..head]);
    if len > SAMPLE {
        let tail = (len - SAMPLE).min(SAMPLE) as usize;
        file.seek(SeekFrom::End(-(tail as i64)))?;
        file.read_exact(&mut buffer[..tail])?;
        hasher.update(&buffer[..tail]);
    }
    ctl.bytes_read.fetch_add(len.min(2 * SAMPLE), Relaxed);
    Ok(*hasher.finalize().as_bytes())
}

fn hash_all(path: &Path, len: u64, ctl: &DuplicateControl) -> io::Result<[u8; 32]> {
    let mut file = open_unchanged(path, len)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; READ_BUFFER];
    let mut total = 0u64;
    loop {
        if ctl.is_cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total += n as u64;
        ctl.bytes_read.fetch_add(n as u64, Relaxed);
    }
    if total != len {
        return Err(io::Error::other("changed while being read"));
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Two paths to one file (hard links) are one copy, not two.
#[cfg(unix)]
fn drop_same_file_twice(members: &mut Vec<Candidate>) {
    use std::os::unix::fs::MetadataExt;
    let mut seen = std::collections::HashSet::new();
    members.retain(|c| match std::fs::metadata(&c.path) {
        Ok(md) => seen.insert((md.dev(), md.ino())),
        Err(_) => false,
    });
}

#[cfg(not(unix))]
fn drop_same_file_twice(_members: &mut Vec<Candidate>) {}
