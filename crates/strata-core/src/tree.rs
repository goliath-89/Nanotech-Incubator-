use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Number of file kinds. Keep in sync with [`Kind`].
pub const KIND_COUNT: usize = 10;

/// What a file is, in the user's terms. The scanner only stores this; deciding
/// it is the job of a [`Classify`] implementation (crate `strata-classify`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Kind {
    #[default]
    Other = 0,
    Videos = 1,
    Photos = 2,
    Music = 3,
    Documents = 4,
    Apps = 5,
    Games = 6,
    Developer = 7,
    System = 8,
    Caches = 9,
}

impl Kind {
    pub const ALL: [Kind; KIND_COUNT] = [
        Kind::Other,
        Kind::Videos,
        Kind::Photos,
        Kind::Music,
        Kind::Documents,
        Kind::Apps,
        Kind::Games,
        Kind::Developer,
        Kind::System,
        Kind::Caches,
    ];

    /// The kind stored as `index`, if there is one.
    pub fn from_index(index: u8) -> Option<Kind> {
        Kind::ALL.get(index as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Other => "Other",
            Kind::Videos => "Videos",
            Kind::Photos => "Photos",
            Kind::Music => "Music",
            Kind::Documents => "Documents",
            Kind::Apps => "Apps",
            Kind::Games => "Games",
            Kind::Developer => "Developer",
            Kind::System => "System",
            Kind::Caches => "Caches",
        }
    }
}

/// Decides the kind of each file while the scan runs. Called from many threads.
pub trait Classify: Sync {
    /// Called once for each folder, before its files. Returning a kind gives
    /// every file directly in that folder that kind (files in a cache folder
    /// are cache, whatever their names) and skips `file_kind` for them.
    fn folder_kind(&self, _dir: &Path) -> Option<Kind> {
        None
    }

    /// The kind of one file, judged by its name.
    fn file_kind(&self, name: &OsStr) -> Kind;
}

/// Classifier that files everything under [`Kind::Other`].
pub struct NoClassify;

impl Classify for NoClassify {
    fn file_kind(&self, _name: &OsStr) -> Kind {
        Kind::Other
    }
}

/// Facts about a folder that change how its numbers should be read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirFlags {
    /// The folder itself could not be listed; its contents are not counted.
    pub unreadable: bool,
    /// The folder is on another file system and was not entered.
    pub other_filesystem: bool,
    /// The folder is on the caller's exclude list and was not entered.
    pub excluded: bool,
    /// The scan was cancelled before this folder was listed.
    pub cancelled: bool,
}

impl DirFlags {
    /// True when the folder's contents were not counted, whatever the reason.
    pub fn not_scanned(self) -> bool {
        self.unreadable || self.other_filesystem || self.excluded || self.cancelled
    }
}

/// One folder. Totals cover the folder and everything below it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirNode {
    /// `None` only for the scan root.
    pub parent: Option<u32>,
    /// Name of the folder (the full root path for the root).
    pub name: OsString,
    /// Child folders, sorted by name.
    pub children: Vec<u32>,
    /// Bytes reserved on disk.
    pub alloc_bytes: u64,
    /// Sum of file lengths. Larger than `alloc_bytes` for sparse or
    /// cloud-only files, smaller for tiny files in large clusters.
    pub logical_bytes: u64,
    /// Files, symbolic links and other non-folder entries.
    pub file_count: u64,
    /// Folders below this one.
    pub dir_count: u64,
    /// `alloc_bytes` of files, split by kind (folders' own blocks excluded).
    pub kind_bytes: [u64; KIND_COUNT],
    /// Length of files that live in the cloud and take no local space.
    pub cloud_only_bytes: u64,
    /// Bytes of the files sitting directly in this folder.
    pub own_files_alloc: u64,
    /// Number of entries sitting directly in this folder that are not folders.
    pub own_file_count: u64,
    pub flags: DirFlags,
}

/// One of the largest files, kept individually for the "largest items" list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileNode {
    /// Index of the folder that holds the file.
    pub dir: u32,
    pub name: OsString,
    pub alloc_bytes: u64,
    pub logical_bytes: u64,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanErrorKind {
    PermissionDenied,
    NotFound,
    Other,
}

/// Something the scanner could not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanError {
    pub path: PathBuf,
    pub kind: ScanErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScanStats {
    /// Extra directory entries that pointed at an already counted file.
    pub hardlink_duplicates: u64,
    /// Bytes that a naive scan would have counted twice.
    pub hardlink_saved_bytes: u64,
    /// Entries that could not be read (may exceed `errors.len()`, which is capped).
    pub error_count: u64,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

/// Result of a scan. Folders are stored parent-first: every folder's index is
/// greater than its parent's, and index 0 is the root.
#[derive(Clone, Debug)]
pub struct ScanTree {
    pub dirs: Vec<DirNode>,
    /// Largest files, biggest first.
    pub files: Vec<FileNode>,
    pub errors: Vec<ScanError>,
    pub stats: ScanStats,
}

impl ScanTree {
    pub fn root(&self) -> &DirNode {
        &self.dirs[0]
    }

    /// Full path of a folder.
    pub fn dir_path(&self, mut id: u32) -> PathBuf {
        let mut parts: Vec<&OsStr> = Vec::new();
        loop {
            let d = &self.dirs[id as usize];
            parts.push(&d.name);
            match d.parent {
                Some(p) => id = p,
                None => break,
            }
        }
        parts.iter().rev().collect()
    }

    /// Full path of one of the kept files.
    pub fn file_path(&self, f: &FileNode) -> PathBuf {
        self.dir_path(f.dir).join(&f.name)
    }
}
