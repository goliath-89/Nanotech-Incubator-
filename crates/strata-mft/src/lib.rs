//! Whole-volume scan by reading the NTFS Master File Table.
//!
//! Every file and folder on an NTFS volume has a record in one table, the
//! MFT. Reading that table from start to end lists the whole volume in one
//! sequential pass, without opening a single folder: several times faster
//! than walking the tree, and it sees folders that an ordinary walk is not
//! allowed to list. It needs raw read access to the volume (administrator
//! rights on Windows).
//!
//! The reader only reads. It works on anything that is `Read + Seek`, so
//! the same code is tested against volume images on any system.
//!
//! Counting rules match `strata-core`:
//! * size on disk is the clusters every stream of the file really occupies,
//!   added up from its run lists (so sparse and compressed files count what
//!   they take, and contents small enough to live inside the file record
//!   count as zero);
//! * a file with several names (hard links) is counted once;
//! * short `8.3` alias names are not entries of their own.

mod parse;

use parse::{attributes, parse_runs, Body, Boot, Run};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;
use strata_core::raw::{build_tree, RawDir, RawFile};
use strata_core::{Classify, Kind, ScanControl, ScanTree};

/// Record number of the volume's root folder.
const ROOT_RECORD: u64 = 5;
/// Record number of `$Extend`, the folder of NTFS's own bookkeeping files.
const EXTEND_RECORD: u64 = 11;
/// Records below this are NTFS's own files (`$MFT`, `$LogFile`, ...).
const FIRST_USER_RECORD: u64 = 24;
/// How much of the table to read at a time.
const CHUNK_BYTES: usize = 4 * 1024 * 1024;
/// Folders can nest thousands deep; building the tree recurses that far.
const BUILD_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Attribute bits as NTFS stores them in a file's standard information.
const DOS_REPARSE_POINT: u32 = 0x0000_0400;
const DOS_OFFLINE: u32 = 0x0000_1000;
const DOS_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// A cloud placeholder (OneDrive and the like) whose contents are not on
/// this disk. On disk, bit 0x40000 means "has extended attributes" (WSL and
/// Docker set it on every file they write); Windows only reuses that value
/// as RECALL_ON_OPEN when it reports attributes to programs, so it must
/// not be read as cloud-only here. Placeholders are always reparse points.
fn is_cloud_only(dos: u32) -> bool {
    dos & DOS_REPARSE_POINT != 0 && dos & (DOS_OFFLINE | DOS_RECALL_ON_DATA_ACCESS) != 0
}

#[derive(Debug)]
pub enum MftError {
    Io(io::Error),
    /// The volume is not NTFS (exFAT, FAT32, ReFS, encrypted, ...).
    NotNtfs,
    /// The file table could not be made sense of.
    Damaged(&'static str),
    /// Raw access to the volume needs administrator rights, and this
    /// program is not running with them.
    NeedsAdministrator,
    /// Raw access was refused although the program runs as administrator.
    /// Security software that guards the disk is the usual cause.
    AccessRefused(io::Error),
    Cancelled,
}

impl std::fmt::Display for MftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MftError::Io(e) => write!(f, "could not read the volume: {e}"),
            MftError::NotNtfs => write!(f, "the volume is not NTFS"),
            MftError::Damaged(what) => write!(f, "the volume's file table is damaged: {what}"),
            MftError::NeedsAdministrator => write!(
                f,
                "reading the drive directly needs administrator rights, and this window does not have them. \
                 Close it, right-click Windows PowerShell (or Terminal), choose Run as administrator, and try again"
            ),
            MftError::AccessRefused(e) => write!(
                f,
                "Windows refused direct access to the drive even though Strata is running as administrator \
                 ({e}). Security software that protects the disk, such as an antivirus, usually causes this"
            ),
            MftError::Cancelled => write!(f, "the scan was cancelled"),
        }
    }
}

impl std::error::Error for MftError {}

impl From<io::Error> for MftError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::PermissionDenied {
            MftError::NeedsAdministrator
        } else {
            MftError::Io(e)
        }
    }
}

#[derive(Clone, Debug)]
pub struct MftOptions {
    /// What to call the root folder in the result, e.g. `C:\`.
    pub root_name: OsString,
    /// Files at least this large are kept individually.
    pub min_file_bytes: u64,
    /// Upper bound on the files kept individually.
    pub max_files: usize,
}

impl MftOptions {
    pub fn new(root_name: impl Into<OsString>) -> Self {
        MftOptions {
            root_name: root_name.into(),
            min_file_bytes: 1024 * 1024,
            max_files: 50_000,
        }
    }
}

/// True if `volume` starts with an NTFS boot sector.
pub fn is_ntfs<R: Read + Seek>(volume: &mut R) -> bool {
    read_boot(volume).is_ok()
}

fn read_boot<R: Read + Seek>(volume: &mut R) -> Result<Boot, MftError> {
    // Raw volume reads must be whole sectors; 4096 covers every sector size.
    let mut sector = [0u8; 4096];
    volume.seek(SeekFrom::Start(0))?;
    volume.read_exact(&mut sector)?;
    parse::parse_boot(&sector).ok_or(MftError::NotNtfs)
}

/// What the table says about one record, kept small: there is one per file.
#[derive(Clone, Copy, Default)]
struct Record {
    on_disk: u64,
    length: u64,
    sequence: u16,
    in_use: bool,
    is_dir: bool,
    cloud_only: bool,
}

struct Link {
    parent: u32,
    parent_sequence: u16,
    child: u32,
    name: Box<str>,
}

/// Scan a whole NTFS volume from its file table.
pub fn scan_volume<R: Read + Seek>(
    mut volume: R,
    opts: &MftOptions,
    classify: &(dyn Classify + Send + Sync),
    ctl: &ScanControl,
) -> Result<ScanTree, MftError> {
    let started = Instant::now();
    let boot = read_boot(&mut volume)?;
    let table = locate_table(&mut volume, &boot)?;
    let record_count = table.length / boot.record_size as u64;
    if record_count <= ROOT_RECORD || record_count > u64::from(u32::MAX) {
        return Err(MftError::Damaged("implausible number of records"));
    }

    let mut records = vec![Record::default(); record_count as usize];
    let mut links: Vec<Link> = Vec::new();
    // Overflow records: extra room for files with many pieces or names.
    // What they hold belongs to their base record.
    let mut overflow: Vec<(u32, u64, u64)> = Vec::new(); // (base, on_disk, length)

    let per_chunk = (CHUNK_BYTES / boot.record_size).max(1) as u64;
    let mut buffer = vec![0u8; per_chunk as usize * boot.record_size];
    let mut next = 0u64;
    while next < record_count {
        if ctl.is_cancelled() {
            return Err(MftError::Cancelled);
        }
        let count = per_chunk.min(record_count - next);
        let bytes = &mut buffer[..count as usize * boot.record_size];
        table.read(&mut volume, &boot, next * boot.record_size as u64, bytes)?;

        let (mut files, mut dirs, mut on_disk) = (0u64, 0u64, 0u64);
        for (i, raw) in bytes.chunks_exact_mut(boot.record_size).enumerate() {
            let number = (next + i as u64) as u32;
            let Some(parsed) = parse_record(raw, number, &boot, &mut links) else {
                continue;
            };
            on_disk += parsed.record.on_disk;
            match parsed.base {
                Some(base) => overflow.push((base, parsed.record.on_disk, parsed.record.length)),
                None => {
                    if parsed.record.is_dir {
                        dirs += 1;
                    } else {
                        files += 1;
                    }
                    records[number as usize] = parsed.record;
                }
            }
        }
        ctl.record(files, dirs, on_disk);
        next += count;
    }

    for (base, on_disk, length) in overflow {
        if let Some(record) = records.get_mut(base as usize) {
            record.on_disk += on_disk;
            record.length += length;
        }
    }

    let (min_file_bytes, max_files) = (opts.min_file_bytes, opts.max_files);
    let root_name = opts.root_name.clone();
    // Tree building recurses as deep as the folders nest, so give it room.
    let mut tree = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("strata-mft-build".into())
            .stack_size(BUILD_STACK_BYTES)
            .spawn_scoped(scope, move || {
                let root = assemble(&records, links, root_name, classify, min_file_bytes);
                build_tree(root, min_file_bytes, max_files)
            })
            .map(|handle| handle.join())
    })?
    .map_err(|_| MftError::Damaged("the folder tree could not be built"))?;

    tree.stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(tree)
}

struct ParsedRecord {
    record: Record,
    /// For an overflow record, the record it belongs to.
    base: Option<u32>,
}

/// Decode one file record. Names found are appended to `links`.
fn parse_record(
    raw: &mut [u8],
    number: u32,
    boot: &Boot,
    links: &mut Vec<Link>,
) -> Option<ParsedRecord> {
    if !parse::apply_fixups(raw) {
        return None; // never used, or not intact
    }
    let flags = parse::u16_at(raw, 0x16)?;
    if flags & 0x01 == 0 {
        return None; // not in use
    }
    let (base_number, _) = parse::split_reference(parse::u64_at(raw, 0x20)?);
    let base = (base_number != 0).then_some(u32::try_from(base_number).ok()?);
    // Names in an overflow record are names of the base record.
    let owner = base.unwrap_or(number);

    let mut record = Record {
        sequence: parse::u16_at(raw, 0x10)?,
        in_use: true,
        is_dir: flags & 0x02 != 0,
        ..Record::default()
    };
    for attribute in attributes(raw) {
        match (&attribute.body, attribute.kind) {
            (Body::Resident(value), parse::ATTR_STANDARD_INFORMATION) => {
                let dos = parse::u32_at(value, 0x20).unwrap_or(0);
                record.cloud_only = is_cloud_only(dos);
            }
            (Body::Resident(value), parse::ATTR_FILE_NAME) => {
                if let Some(name) = parse::parse_file_name(value) {
                    if !name.dos_only {
                        if let Ok(parent) = u32::try_from(name.parent) {
                            links.push(Link {
                                parent,
                                parent_sequence: name.parent_sequence,
                                child: owner,
                                name: name.name.into_boxed_str(),
                            });
                        }
                    }
                }
            }
            (Body::Resident(value), parse::ATTR_DATA) if !attribute.named => {
                // Contents stored inside the record: a length, but no clusters.
                record.length += value.len() as u64;
            }
            (
                Body::NonResident {
                    first_vcn,
                    runs,
                    length,
                },
                kind,
            ) => {
                // Every stream that owns clusters counts towards space on
                // disk, piece by piece. A run list that claims more than
                // the volume holds is damage, and counts as nothing.
                let clusters = parse::clusters_in_runs(runs)
                    .filter(|c| *c <= boot.total_clusters)
                    .unwrap_or(0);
                record.on_disk = record.on_disk.saturating_add(clusters * boot.cluster_size);
                // Only the main contents count towards the length, and the
                // length is written on the stream's first piece.
                if kind == parse::ATTR_DATA && !attribute.named && *first_vcn == 0 {
                    record.length = record.length.saturating_add(*length);
                }
            }
            _ => {}
        }
    }
    Some(ParsedRecord { record, base })
}

/// Where the file table itself lives on the volume.
struct Table {
    runs: Vec<Run>,
    length: u64,
}

impl Table {
    /// Read `out.len()` bytes of the table starting at byte `offset` of it.
    fn read<R: Read + Seek>(
        &self,
        volume: &mut R,
        boot: &Boot,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), MftError> {
        let mut done = 0usize;
        while done < out.len() {
            let position = offset + done as u64;
            let vcn = position / boot.cluster_size;
            // Runs are sorted by position in the table.
            let index = self.runs.partition_point(|r| r.vcn + r.len <= vcn);
            let run = self
                .runs
                .get(index)
                .filter(|r| r.vcn <= vcn)
                .ok_or(MftError::Damaged("the file table has a gap"))?;
            let within = position - run.vcn * boot.cluster_size;
            let available = run.len * boot.cluster_size - within;
            let take = (out.len() - done).min(available.min(usize::MAX as u64) as usize);
            let disk_offset = run
                .lcn
                .checked_mul(boot.cluster_size)
                .and_then(|o| o.checked_add(within))
                .ok_or(MftError::Damaged(
                    "the file table points outside the volume",
                ))?;
            volume.seek(SeekFrom::Start(disk_offset))?;
            volume.read_exact(&mut out[done..done + take])?;
            done += take;
        }
        Ok(())
    }
}

/// Find every piece of the file table. Its first record describes it; on a
/// badly fragmented volume the description continues in overflow records,
/// which sit in pieces already known.
fn locate_table<R: Read + Seek>(volume: &mut R, boot: &Boot) -> Result<Table, MftError> {
    let mut first = vec![0u8; boot.record_size];
    let start = boot
        .mft_lcn
        .checked_mul(boot.cluster_size)
        .ok_or(MftError::NotNtfs)?;
    volume.seek(SeekFrom::Start(start))?;
    volume.read_exact(&mut first)?;
    if !parse::apply_fixups(&mut first) {
        return Err(MftError::Damaged("the first record is not intact"));
    }

    let mut table = Table {
        runs: Vec::new(),
        length: 0,
    };
    let mut list: Option<Vec<u8>> = None;
    for attribute in attributes(&first) {
        match (attribute.kind, &attribute.body) {
            (
                parse::ATTR_DATA,
                Body::NonResident {
                    first_vcn: 0,
                    runs,
                    length,
                    ..
                },
            ) if !attribute.named => {
                table.runs = parse_runs(runs, 0).ok_or(MftError::Damaged("unreadable run list"))?;
                table.length = *length;
            }
            (parse::ATTR_ATTRIBUTE_LIST, Body::Resident(value)) => list = Some(value.to_vec()),
            (parse::ATTR_ATTRIBUTE_LIST, Body::NonResident { runs, length, .. }) => {
                let runs = parse_runs(runs, 0).ok_or(MftError::Damaged("unreadable run list"))?;
                list = Some(read_stream(volume, boot, &runs, *length)?);
            }
            _ => {}
        }
    }
    if table.runs.is_empty() || table.length == 0 {
        return Err(MftError::Damaged("the file table does not describe itself"));
    }

    if let Some(list) = list {
        let mut pieces: Vec<_> = parse::parse_attribute_list(&list)
            .into_iter()
            .filter(|e| e.kind == parse::ATTR_DATA && !e.named && e.first_vcn > 0)
            .collect();
        pieces.sort_by_key(|e| e.first_vcn);
        let mut record = vec![0u8; boot.record_size];
        for piece in pieces {
            // Each further piece is described in a record inside the pieces found so far.
            table.read(
                volume,
                boot,
                piece.record * boot.record_size as u64,
                &mut record,
            )?;
            if !parse::apply_fixups(&mut record) {
                return Err(MftError::Damaged(
                    "an overflow record of the file table is not intact",
                ));
            }
            for attribute in attributes(&record) {
                if let (
                    parse::ATTR_DATA,
                    false,
                    Body::NonResident {
                        first_vcn, runs, ..
                    },
                ) = (attribute.kind, attribute.named, &attribute.body)
                {
                    if *first_vcn == piece.first_vcn {
                        let more = parse_runs(runs, *first_vcn)
                            .ok_or(MftError::Damaged("unreadable run list"))?;
                        table.runs.extend(more);
                    }
                }
            }
            table.runs.sort_by_key(|r| r.vcn);
        }
    }
    // Refuse anything that points past the end of the volume.
    if table.runs.iter().any(|r| {
        r.lcn
            .checked_add(r.len)
            .is_none_or(|end| end > boot.total_clusters + 1)
    }) {
        return Err(MftError::Damaged(
            "the file table points outside the volume",
        ));
    }
    Ok(table)
}

fn read_stream<R: Read + Seek>(
    volume: &mut R,
    boot: &Boot,
    runs: &[Run],
    length: u64,
) -> Result<Vec<u8>, MftError> {
    if length > 64 * 1024 * 1024 {
        return Err(MftError::Damaged("implausibly large attribute list"));
    }
    let stream = Table {
        runs: runs.to_vec(),
        length,
    };
    let mut out = vec![0u8; length as usize];
    stream.read(volume, boot, 0, &mut out)?;
    Ok(out)
}

/// Turn the flat list of records and names into folders.
fn assemble(
    records: &[Record],
    mut links: Vec<Link>,
    root_name: OsString,
    classify: &dyn Classify,
    min_file_bytes: u64,
) -> RawDir {
    // A name counts only if the folder it claims to be in still exists and
    // has not been reused since (its sequence number matches).
    links.retain(|l| {
        let parent = records.get(l.parent as usize);
        let child = records.get(l.child as usize);
        matches!((parent, child), (Some(p), Some(c)) if p.in_use && p.is_dir && p.sequence == l.parent_sequence && c.in_use)
            && u64::from(l.child) != ROOT_RECORD
    });
    let mut names_per_file: HashMap<u32, u32> = HashMap::new();
    for l in &links {
        if !records[l.child as usize].is_dir {
            *names_per_file.entry(l.child).or_default() += 1;
        }
    }
    names_per_file.retain(|_, n| *n > 1);

    links.sort_by(|a, b| a.parent.cmp(&b.parent).then_with(|| a.name.cmp(&b.name)));
    let mut visited = vec![false; records.len()];
    let state = Assemble {
        records,
        links: &links,
        names_per_file: &names_per_file,
        classify,
        min_file_bytes,
    };
    let root_path = PathBuf::from(&root_name);
    state.folder(
        ROOT_RECORD as u32,
        root_name,
        &root_path,
        false,
        &mut visited,
    )
}

struct Assemble<'a> {
    records: &'a [Record],
    links: &'a [Link],
    names_per_file: &'a HashMap<u32, u32>,
    classify: &'a dyn Classify,
    min_file_bytes: u64,
}

impl Assemble<'_> {
    fn folder(
        &self,
        number: u32,
        name: OsString,
        path: &Path,
        system: bool,
        visited: &mut [bool],
    ) -> RawDir {
        visited[number as usize] = true;
        let mut dir = RawDir::new(name, self.records[number as usize].on_disk);
        let folder_kind = if system {
            Some(Kind::System)
        } else {
            self.classify.folder_kind(path)
        };

        let first = self.links.partition_point(|l| l.parent < number);
        for link in self.links[first..]
            .iter()
            .take_while(|l| l.parent == number)
        {
            let child = self.records[link.child as usize];
            let child_name = OsString::from(&*link.name);
            if child.is_dir {
                // A folder has one place. A second claim (a damaged table,
                // or a loop) is ignored.
                if visited[link.child as usize] {
                    continue;
                }
                let child_path = path.join(&child_name);
                let child_system = system || u64::from(link.child) == EXTEND_RECORD;
                dir.children.push(self.folder(
                    link.child,
                    child_name,
                    &child_path,
                    child_system,
                    visited,
                ));
                continue;
            }
            // NTFS's own files (`$MFT`, `$LogFile`, the change journal) are system.
            let own = system || u64::from(link.child) < FIRST_USER_RECORD;
            let kind = if own {
                Kind::System
            } else {
                folder_kind.unwrap_or_else(|| self.classify.file_kind(&child_name))
            };
            if child.cloud_only {
                dir.cloud_only += child.length;
            }
            let file = RawFile {
                name: child_name,
                alloc: child.on_disk,
                logical: child.length,
                kind,
            };
            if self.names_per_file.contains_key(&link.child) {
                dir.add_link((0, u64::from(link.child)), file);
            } else {
                dir.add_file(file, self.min_file_bytes);
            }
        }
        dir
    }
}

/// Open a Windows drive for raw reading, e.g. `open_drive('C')`.
/// Needs administrator rights.
#[cfg(windows)]
pub fn open_drive(letter: char) -> Result<std::fs::File, MftError> {
    if !letter.is_ascii_alphabetic() {
        return Err(MftError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a drive letter",
        )));
    }
    let path = format!(r"\\.\{}:", letter.to_ascii_uppercase());
    match std::fs::File::open(path) {
        Ok(file) => Ok(file),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            if is_elevated() {
                Err(MftError::AccessRefused(e))
            } else {
                Err(MftError::NeedsAdministrator)
            }
        }
        Err(e) => Err(MftError::Io(e)),
    }
}

/// True when this process runs with administrator rights (an elevated token).
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing;
    // `token` is a valid out-pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut written = 0u32;
    // SAFETY: `token` was opened above with TOKEN_QUERY; the buffer is a
    // TOKEN_ELEVATION of the size passed; `written` is a valid out-pointer.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut written,
        )
    };
    // SAFETY: `token` is a handle this function opened and owns.
    unsafe { CloseHandle(token) };
    ok != 0 && elevation.TokenIsElevated != 0
}

#[cfg(test)]
mod tests {
    use super::is_cloud_only;

    #[test]
    fn extended_attributes_are_not_cloud_placeholders() {
        // A file written by WSL or Docker: archive + has-EA.
        assert!(!is_cloud_only(0x0000_0020 | 0x0004_0000));
        // Offline without a reparse point is not a cloud placeholder either.
        assert!(!is_cloud_only(0x0000_1000));
        // OneDrive "online only": reparse point + recall on data access.
        assert!(is_cloud_only(0x0000_0400 | 0x0040_0000));
        assert!(is_cloud_only(0x0000_0400 | 0x0000_1000 | 0x0040_0000));
    }
}
