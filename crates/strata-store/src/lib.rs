//! Saved scans.
//!
//! One SQLite file holds every scan the user has kept. A scan is written in a
//! single transaction, so a crash or power cut leaves either the whole scan
//! or none of it. Old scans can be pruned down to a small snapshot of folder
//! sizes, which is all the Growth view needs.

#![forbid(unsafe_code)]

mod names;
mod schema;

use rusqlite::{params, Connection, OptionalExtension, Row};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use strata_core::{
    DirFlags, DirNode, FileNode, Kind, ScanError, ScanErrorKind, ScanStats, ScanTree, KIND_COUNT,
};

/// Folders deeper than this are left out of growth snapshots.
pub const SNAPSHOT_MAX_DEPTH: u32 = 6;
/// Folders smaller than this are left out of growth snapshots.
pub const SNAPSHOT_MIN_BYTES: u64 = 1024 * 1024;

pub type ScanId = i64;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// No scan, or no folder, with that id.
    NotFound,
    /// The scan's folders were pruned; only its totals and snapshot remain.
    TreePruned,
    /// The file was written by a newer version of Strata.
    NewerSchema {
        found: i64,
        supported: i64,
    },
    /// The database holds something this version cannot make sense of.
    Corrupt(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "database error: {e}"),
            StoreError::NotFound => write!(f, "no such scan or folder"),
            StoreError::TreePruned => write!(f, "this scan's folder details were pruned"),
            StoreError::NewerSchema { found, supported } => write!(
                f,
                "the scan history was written by a newer version of Strata (format {found}, this version reads up to {supported})"
            ),
            StoreError::Corrupt(what) => write!(f, "the scan history is damaged: {what}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Sqlite(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// What the caller knows about a scan that the tree itself does not carry.
#[derive(Clone, Debug)]
pub struct ScanMeta {
    /// Stable identity of the volume, so scans of the same drive line up.
    pub volume_id: String,
    /// Seconds since 1970-01-01 UTC.
    pub started_at: i64,
    pub finished_at: i64,
}

/// One saved scan, without its folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanInfo {
    pub id: ScanId,
    pub volume_id: String,
    pub root: PathBuf,
    pub started_at: i64,
    pub finished_at: i64,
    pub alloc_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub error_count: u64,
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// False once the folders were pruned.
    pub has_tree: bool,
}

/// One folder as stored. Totals cover the folder and everything below it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirRow {
    pub id: u32,
    pub parent: Option<u32>,
    pub name: OsString,
    pub alloc_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub kind_bytes: [u64; KIND_COUNT],
    pub cloud_only_bytes: u64,
    pub own_files_alloc: u64,
    pub own_file_count: u64,
    pub flags: DirFlags,
    /// The app this folder belongs to, once [`Store::save_apps`] has run.
    pub owner: Option<DirOwner>,
}

/// What a folder is to the app that owns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppRole {
    /// The program itself.
    Program,
    /// What it created and needs.
    Data,
    /// What it rebuilds on its own.
    Cache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirOwner {
    /// Position in [`Store::apps`] for the same scan.
    pub app: u32,
    pub role: AppRole,
}

/// One app's share of a scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppRow {
    pub name: String,
    /// Stable id from the app rules.
    pub key: String,
    pub kind: Option<Kind>,
    pub app_bytes: u64,
    pub data_bytes: u64,
    pub cache_bytes: u64,
}

/// One cleanup suggestion, as computed right after the scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionRow {
    /// Which heading it belongs under; a stable key such as `app_cache`.
    pub group: String,
    /// True: empty the folder at `path` and keep it. False: `path` itself.
    pub contents_only: bool,
    pub path: PathBuf,
    pub bytes: u64,
    pub app: Option<String>,
    pub rebuildable: bool,
    pub reason: String,
}

/// One of the largest files as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub dir_id: u32,
    pub name: OsString,
    pub alloc_bytes: u64,
    pub logical_bytes: u64,
    pub kind: Kind,
}

/// A folder and what is directly inside it: enough to draw one level.
#[derive(Clone, Debug)]
pub struct DirListing {
    pub dir: DirRow,
    /// From the scan root down to `dir`, both included.
    pub breadcrumb: Vec<(u32, OsString)>,
    /// Child folders, largest first.
    pub children: Vec<DirRow>,
    /// Large files directly in this folder, largest first.
    pub files: Vec<FileRow>,
}

/// How one folder changed between two scans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrowthRow {
    /// Relative to the scan root, `/` separated; empty for the root itself.
    pub path: String,
    pub depth: u32,
    pub before_bytes: u64,
    pub after_bytes: u64,
}

impl GrowthRow {
    pub fn delta(&self) -> i64 {
        self.after_bytes as i64 - self.before_bytes as i64
    }
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open the scan history at `path`, creating it if needed.
    pub fn open(path: &Path) -> Result<Store> {
        let conn = Connection::open(path)?;
        // WAL keeps readers working while a scan is being saved.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Store::init(conn)
    }

    /// A store that lives only in memory, for tests.
    pub fn open_in_memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Store> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let found = schema::migrate(&mut conn)?;
        if found > schema::SCHEMA_VERSION {
            return Err(StoreError::NewerSchema {
                found,
                supported: schema::SCHEMA_VERSION,
            });
        }
        Ok(Store { conn })
    }

    /// Save a finished scan. All or nothing.
    pub fn save(&mut self, tree: &ScanTree, meta: &ScanMeta) -> Result<ScanId> {
        let root = tree.root();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO scans (volume_id, root, started_at, finished_at, alloc_bytes, logical_bytes,
                                file_count, dir_count, error_count, cancelled, elapsed_ms,
                                hardlink_duplicates, hardlink_saved_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                meta.volume_id,
                names::encode(&root.name),
                meta.started_at,
                meta.finished_at,
                int(root.alloc_bytes),
                int(root.logical_bytes),
                int(root.file_count),
                int(root.dir_count),
                int(tree.stats.error_count),
                tree.stats.cancelled,
                int(tree.stats.elapsed_ms),
                int(tree.stats.hardlink_duplicates),
                int(tree.stats.hardlink_saved_bytes),
            ],
        )?;
        let scan = tx.last_insert_rowid();

        {
            let mut insert = tx.prepare(
                "INSERT INTO dirs (scan_id, id, parent_id, name, alloc_bytes, logical_bytes, file_count,
                                   dir_count, kind_bytes, cloud_only_bytes, own_files_alloc,
                                   own_file_count, flags)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )?;
            for (id, d) in tree.dirs.iter().enumerate() {
                insert.execute(params![
                    scan,
                    id as i64,
                    d.parent,
                    names::encode(&d.name),
                    int(d.alloc_bytes),
                    int(d.logical_bytes),
                    int(d.file_count),
                    int(d.dir_count),
                    kinds_to_blob(&d.kind_bytes),
                    int(d.cloud_only_bytes),
                    int(d.own_files_alloc),
                    int(d.own_file_count),
                    flags_to_bits(d.flags),
                ])?;
            }

            let mut insert = tx.prepare(
                "INSERT INTO files (scan_id, rank, dir_id, name, alloc_bytes, logical_bytes, kind)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (rank, f) in tree.files.iter().enumerate() {
                insert.execute(params![
                    scan,
                    rank as i64,
                    f.dir,
                    names::encode(&f.name),
                    int(f.alloc_bytes),
                    int(f.logical_bytes),
                    f.kind as u8,
                ])?;
            }

            let mut insert = tx.prepare(
                "INSERT INTO scan_errors (scan_id, seq, path, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (seq, e) in tree.errors.iter().enumerate() {
                insert.execute(params![
                    scan,
                    seq as i64,
                    names::encode(e.path.as_os_str()),
                    error_kind_to_int(e.kind),
                    e.message,
                ])?;
            }

            let mut insert = tx.prepare(
                "INSERT INTO snapshots (scan_id, path, depth, alloc_bytes) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (path, depth, bytes) in snapshot_rows(tree) {
                insert.execute(params![scan, path, depth, int(bytes)])?;
            }
        }

        tx.commit()?;
        Ok(scan)
    }

    /// Saved scans, newest first. `volume_id` narrows the list to one drive.
    pub fn scans(&self, volume_id: Option<&str>) -> Result<Vec<ScanInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, volume_id, root, started_at, finished_at, alloc_bytes, logical_bytes, file_count,
                    dir_count, error_count, cancelled, elapsed_ms, has_tree
             FROM scans WHERE ?1 IS NULL OR volume_id = ?1
             ORDER BY finished_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![volume_id], scan_info_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The most recent scan of a volume, if any.
    pub fn latest(&self, volume_id: &str) -> Result<Option<ScanInfo>> {
        Ok(self.scans(Some(volume_id))?.into_iter().next())
    }

    pub fn scan_info(&self, scan: ScanId) -> Result<ScanInfo> {
        self.conn
            .query_row(
                "SELECT id, volume_id, root, started_at, finished_at, alloc_bytes, logical_bytes, file_count,
                        dir_count, error_count, cancelled, elapsed_ms, has_tree
                 FROM scans WHERE id = ?1",
                params![scan],
                scan_info_from_row,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    /// Read a whole scan back, exactly as it was saved.
    pub fn load(&self, scan: ScanId) -> Result<ScanTree> {
        let info = self.require_tree(scan)?;

        let mut dirs: Vec<DirNode> = Vec::with_capacity(info.dir_count as usize + 1);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {DIR_COLUMNS} FROM dirs WHERE scan_id = ?1 ORDER BY id"
        ))?;
        let mut rows = stmt.query(params![scan])?;
        while let Some(row) = rows.next()? {
            let d = dir_from_row(row)?;
            if d.id as usize != dirs.len() {
                return Err(StoreError::Corrupt(format!(
                    "folder {} is missing",
                    dirs.len()
                )));
            }
            match d.parent {
                // Ids were handed out parent-first with children in name
                // order, so appending in id order restores that order.
                Some(p) if (p as usize) < dirs.len() => dirs[p as usize].children.push(d.id),
                None if d.id == 0 => {}
                _ => {
                    return Err(StoreError::Corrupt(format!(
                        "folder {} has a bad parent",
                        d.id
                    )))
                }
            }
            dirs.push(DirNode {
                parent: d.parent,
                name: d.name,
                children: Vec::new(),
                alloc_bytes: d.alloc_bytes,
                logical_bytes: d.logical_bytes,
                file_count: d.file_count,
                dir_count: d.dir_count,
                kind_bytes: d.kind_bytes,
                cloud_only_bytes: d.cloud_only_bytes,
                own_files_alloc: d.own_files_alloc,
                own_file_count: d.own_file_count,
                flags: d.flags,
            });
        }
        if dirs.is_empty() {
            return Err(StoreError::Corrupt("the scan has no root folder".into()));
        }

        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE scan_id = ?1 ORDER BY rank"
        ))?;
        let files = stmt
            .query_map(params![scan], file_from_row)?
            .map(|r| {
                r.map(|f| FileNode {
                    dir: f.dir_id,
                    name: f.name,
                    alloc_bytes: f.alloc_bytes,
                    logical_bytes: f.logical_bytes,
                    kind: f.kind,
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let (hardlink_duplicates, hardlink_saved_bytes): (i64, i64) = self.conn.query_row(
            "SELECT hardlink_duplicates, hardlink_saved_bytes FROM scans WHERE id = ?1",
            params![scan],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        Ok(ScanTree {
            dirs,
            files,
            errors: self.errors(scan)?,
            stats: ScanStats {
                hardlink_duplicates: hardlink_duplicates as u64,
                hardlink_saved_bytes: hardlink_saved_bytes as u64,
                error_count: info.error_count,
                cancelled: info.cancelled,
                elapsed_ms: info.elapsed_ms,
            },
        })
    }

    /// One folder, the path leading to it, and what is directly inside it.
    /// This is what a view needs to draw a level without loading the scan.
    pub fn children(&self, scan: ScanId, dir: u32) -> Result<DirListing> {
        self.require_tree(scan)?;
        let this = self.dir(scan, dir)?;

        let mut breadcrumb = vec![(this.id, this.name.clone())];
        let mut up = this.parent;
        while let Some(p) = up {
            let (name, parent): (Vec<u8>, Option<u32>) = self
                .conn
                .query_row(
                    "SELECT name, parent_id FROM dirs WHERE scan_id = ?1 AND id = ?2",
                    params![scan, p],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| StoreError::Corrupt(format!("folder {p} is missing")))?;
            breadcrumb.push((p, names::decode(name)));
            up = parent;
        }
        breadcrumb.reverse();

        let children = self.child_dirs(scan, dir)?;

        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE scan_id = ?1 AND dir_id = ?2 ORDER BY rank"
        ))?;
        let files = stmt
            .query_map(params![scan, dir], file_from_row)?
            .collect::<rusqlite::Result<_>>()?;

        Ok(DirListing {
            dir: this,
            breadcrumb,
            children,
            files,
        })
    }

    /// The folders directly inside `dir`, largest first. Cheaper than
    /// [`Store::children`] when only the folders are needed (drawing the
    /// ring several levels deep).
    pub fn child_dirs(&self, scan: ScanId, dir: u32) -> Result<Vec<DirRow>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {DIR_COLUMNS} FROM dirs WHERE scan_id = ?1 AND parent_id = ?2
             ORDER BY alloc_bytes DESC, id"
        ))?;
        let dirs = collect_dirs(stmt.query(params![scan, dir])?)?;
        Ok(dirs)
    }

    /// The folders that belong to app number `app` (its position in
    /// [`Store::apps`]), outermost only: a folder whose parent belongs to
    /// the same app is left out. Largest first, at most `limit`.
    pub fn app_dirs(&self, scan: ScanId, app: u32, limit: usize) -> Result<Vec<DirRow>> {
        self.require_tree(scan)?;
        let columns: String = DIR_COLUMNS
            .split(", ")
            .map(|c| format!("d.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {columns} FROM dirs d
             LEFT JOIN dirs p ON p.scan_id = d.scan_id AND p.id = d.parent_id
             WHERE d.scan_id = ?1 AND d.app_idx = ?2 AND (p.app_idx IS NULL OR p.app_idx != ?2)
             ORDER BY d.alloc_bytes DESC, d.id LIMIT ?3"
        ))?;
        let dirs =
            collect_dirs(stmt.query(params![scan, app, limit.min(i64::MAX as usize) as i64])?)?;
        Ok(dirs)
    }

    /// Full path of a saved folder.
    pub fn dir_path(&self, scan: ScanId, dir: u32) -> Result<PathBuf> {
        Ok(self
            .children(scan, dir)?
            .breadcrumb
            .iter()
            .map(|(_, name)| name)
            .collect())
    }

    /// The largest files of a scan, optionally of one kind, largest first.
    pub fn largest_files(
        &self,
        scan: ScanId,
        kind: Option<Kind>,
        limit: usize,
    ) -> Result<Vec<FileRow>> {
        self.require_tree(scan)?;
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {FILE_COLUMNS} FROM files
             WHERE scan_id = ?1 AND (?2 IS NULL OR kind = ?2) ORDER BY rank LIMIT ?3"
        ))?;
        let rows = stmt.query_map(
            params![
                scan,
                kind.map(|k| k as u8),
                limit.min(i64::MAX as usize) as i64
            ],
            file_from_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// What could not be read during a scan.
    pub fn errors(&self, scan: ScanId) -> Result<Vec<ScanError>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT path, kind, message FROM scan_errors WHERE scan_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![scan], |r| {
            Ok(ScanError {
                path: PathBuf::from(names::decode(r.get(0)?)),
                kind: error_kind_from_int(r.get(1)?),
                message: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Folders whose size differs between two scans, biggest change first.
    /// Works on pruned scans too. A folder and its parents each get a row;
    /// `depth` lets the caller pick the level it wants to show.
    pub fn diff(&self, before: ScanId, after: ScanId, limit: usize) -> Result<Vec<GrowthRow>> {
        self.scan_info(before)?;
        self.scan_info(after)?;
        let mut stmt = self.conn.prepare_cached(
            "WITH a AS (SELECT path, MIN(depth) AS depth, SUM(alloc_bytes) AS bytes
                        FROM snapshots WHERE scan_id = ?1 GROUP BY path),
                  b AS (SELECT path, MIN(depth) AS depth, SUM(alloc_bytes) AS bytes
                        FROM snapshots WHERE scan_id = ?2 GROUP BY path)
             SELECT a.path, a.depth, a.bytes, COALESCE(b.bytes, 0) FROM a LEFT JOIN b ON b.path = a.path
             UNION ALL
             SELECT b.path, b.depth, 0, b.bytes FROM b LEFT JOIN a ON a.path = b.path WHERE a.path IS NULL",
        )?;
        let mut rows: Vec<GrowthRow> = stmt
            .query_map(params![before, after], |r| {
                Ok(GrowthRow {
                    path: r.get(0)?,
                    depth: r.get(1)?,
                    before_bytes: r.get::<_, i64>(2)? as u64,
                    after_bytes: r.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.retain(|r| r.delta() != 0);
        rows.sort_by(|x, y| {
            y.delta()
                .unsigned_abs()
                .cmp(&x.delta().unsigned_abs())
                .then_with(|| x.depth.cmp(&y.depth))
                .then_with(|| x.path.cmp(&y.path))
        });
        rows.truncate(limit);
        Ok(rows)
    }

    /// Remove a scan completely, snapshot included.
    pub fn delete(&mut self, scan: ScanId) -> Result<()> {
        // Child rows go with it (ON DELETE CASCADE).
        match self
            .conn
            .execute("DELETE FROM scans WHERE id = ?1", params![scan])?
        {
            0 => Err(StoreError::NotFound),
            _ => Ok(()),
        }
    }

    /// Keep the folder details of only the newest `keep` scans of a volume.
    /// Older scans keep their totals and growth snapshot. Returns how many
    /// scans were slimmed down.
    pub fn prune(&mut self, volume_id: &str, keep: usize) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let old: Vec<ScanId> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM scans WHERE volume_id = ?1 AND has_tree = 1
                 ORDER BY finished_at DESC, id DESC LIMIT -1 OFFSET ?2",
            )?;
            let ids = stmt.query_map(params![volume_id, keep as i64], |r| r.get(0))?;
            ids.collect::<rusqlite::Result<_>>()?
        };
        for id in &old {
            for table in ["dirs", "files", "scan_errors", "suggestions"] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE scan_id = ?1"),
                    params![id],
                )?;
            }
            tx.execute("UPDATE scans SET has_tree = 0 WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(old.len())
    }

    /// Record which app each folder of a scan belongs to. `owners` has one
    /// entry per folder, in folder-id order. Replaces what was there.
    pub fn save_apps(
        &mut self,
        scan: ScanId,
        apps: &[AppRow],
        owners: &[Option<DirOwner>],
    ) -> Result<()> {
        let info = self.require_tree(scan)?;
        if owners.len() as u64 != info.dir_count + 1 {
            return Err(StoreError::Corrupt(format!(
                "{} owners for {} folders",
                owners.len(),
                info.dir_count + 1
            )));
        }
        if owners
            .iter()
            .flatten()
            .any(|o| o.app as usize >= apps.len())
        {
            return Err(StoreError::Corrupt(
                "a folder names an app that is not listed".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM scan_apps WHERE scan_id = ?1", params![scan])?;
        tx.execute(
            "UPDATE dirs SET app_idx = NULL, app_role = NULL WHERE scan_id = ?1 AND app_idx IS NOT NULL",
            params![scan],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO scan_apps (scan_id, idx, name, key, kind, app_bytes, data_bytes, cache_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (idx, a) in apps.iter().enumerate() {
                insert.execute(params![
                    scan,
                    idx as i64,
                    a.name,
                    a.key,
                    a.kind.map(|k| k as u8),
                    int(a.app_bytes),
                    int(a.data_bytes),
                    int(a.cache_bytes),
                ])?;
            }
            let mut update = tx.prepare(
                "UPDATE dirs SET app_idx = ?3, app_role = ?4 WHERE scan_id = ?1 AND id = ?2",
            )?;
            for (dir, owner) in owners.iter().enumerate() {
                if let Some(o) = owner {
                    update.execute(params![scan, dir as i64, o.app, role_to_int(o.role)])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The apps that own space in a scan, in the order they were saved
    /// (largest first). Kept when the scan's folders are pruned.
    pub fn apps(&self, scan: ScanId) -> Result<Vec<AppRow>> {
        self.scan_info(scan)?;
        let mut stmt = self.conn.prepare_cached(
            "SELECT name, key, kind, app_bytes, data_bytes, cache_bytes FROM scan_apps
             WHERE scan_id = ?1 ORDER BY idx",
        )?;
        let rows = stmt.query_map(params![scan], |r| {
            Ok(AppRow {
                name: r.get(0)?,
                key: r.get(1)?,
                kind: r.get::<_, Option<u8>>(2)?.and_then(Kind::from_index),
                app_bytes: r.get::<_, i64>(3)? as u64,
                data_bytes: r.get::<_, i64>(4)? as u64,
                cache_bytes: r.get::<_, i64>(5)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Record a scan's cleanup suggestions, in display order. Replaces what
    /// was there.
    pub fn save_suggestions(&mut self, scan: ScanId, suggestions: &[SuggestionRow]) -> Result<()> {
        self.require_tree(scan)?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM suggestions WHERE scan_id = ?1", params![scan])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO suggestions (scan_id, seq, grp, contents_only, path, bytes, app, rebuildable, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for (seq, s) in suggestions.iter().enumerate() {
                insert.execute(params![
                    scan,
                    seq as i64,
                    s.group,
                    s.contents_only,
                    names::encode(s.path.as_os_str()),
                    int(s.bytes),
                    s.app,
                    s.rebuildable,
                    s.reason,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// A scan's cleanup suggestions as saved. They describe the disk as it
    /// was at scan time; check each again before acting on it.
    pub fn suggestions(&self, scan: ScanId) -> Result<Vec<SuggestionRow>> {
        self.scan_info(scan)?;
        let mut stmt = self.conn.prepare_cached(
            "SELECT grp, contents_only, path, bytes, app, rebuildable, reason FROM suggestions
             WHERE scan_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![scan], |r| {
            Ok(SuggestionRow {
                group: r.get(0)?,
                contents_only: r.get(1)?,
                path: PathBuf::from(names::decode(r.get(2)?)),
                bytes: r.get::<_, i64>(3)? as u64,
                app: r.get(4)?,
                rebuildable: r.get(5)?,
                reason: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Suggestions with the number each was saved under, for acting on them
    /// one by one.
    pub fn numbered_suggestions(&self, scan: ScanId) -> Result<Vec<(u32, SuggestionRow)>> {
        self.scan_info(scan)?;
        let mut stmt = self.conn.prepare_cached(
            "SELECT seq, grp, contents_only, path, bytes, app, rebuildable, reason FROM suggestions
             WHERE scan_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![scan], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                SuggestionRow {
                    group: r.get(1)?,
                    contents_only: r.get(2)?,
                    path: PathBuf::from(names::decode(r.get(3)?)),
                    bytes: r.get::<_, i64>(4)? as u64,
                    app: r.get(5)?,
                    rebuildable: r.get(6)?,
                    reason: r.get(7)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Forget suggestions that have been acted on.
    pub fn remove_suggestions(&mut self, scan: ScanId, numbers: &[u32]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for n in numbers {
            tx.execute(
                "DELETE FROM suggestions WHERE scan_id = ?1 AND seq = ?2",
                params![scan, n],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn require_tree(&self, scan: ScanId) -> Result<ScanInfo> {
        let info = self.scan_info(scan)?;
        if info.has_tree {
            Ok(info)
        } else {
            Err(StoreError::TreePruned)
        }
    }

    fn dir(&self, scan: ScanId, dir: u32) -> Result<DirRow> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {DIR_COLUMNS} FROM dirs WHERE scan_id = ?1 AND id = ?2"
        ))?;
        let mut found = collect_dirs(stmt.query(params![scan, dir])?)?;
        found.pop().ok_or(StoreError::NotFound)
    }
}

const DIR_COLUMNS: &str =
    "id, parent_id, name, alloc_bytes, logical_bytes, file_count, dir_count, \
     kind_bytes, cloud_only_bytes, own_files_alloc, own_file_count, flags, app_idx, app_role";
const FILE_COLUMNS: &str = "dir_id, name, alloc_bytes, logical_bytes, kind";

fn collect_dirs(mut rows: rusqlite::Rows<'_>) -> Result<Vec<DirRow>> {
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(dir_from_row(row)?);
    }
    Ok(out)
}

fn dir_from_row(r: &Row<'_>) -> Result<DirRow> {
    let id: u32 = r.get(0)?;
    let blob: Vec<u8> = r.get(7)?;
    Ok(DirRow {
        id,
        parent: r.get(1)?,
        name: names::decode(r.get(2)?),
        alloc_bytes: r.get::<_, i64>(3)? as u64,
        logical_bytes: r.get::<_, i64>(4)? as u64,
        file_count: r.get::<_, i64>(5)? as u64,
        dir_count: r.get::<_, i64>(6)? as u64,
        kind_bytes: kinds_from_blob(&blob)
            .ok_or_else(|| StoreError::Corrupt(format!("folder {id} has bad kind totals")))?,
        cloud_only_bytes: r.get::<_, i64>(8)? as u64,
        own_files_alloc: r.get::<_, i64>(9)? as u64,
        own_file_count: r.get::<_, i64>(10)? as u64,
        flags: flags_from_bits(r.get(11)?),
        owner: match (r.get::<_, Option<u32>>(12)?, r.get::<_, Option<u8>>(13)?) {
            (Some(app), Some(role)) => Some(DirOwner {
                app,
                role: role_from_int(role),
            }),
            _ => None,
        },
    })
}

fn file_from_row(r: &Row<'_>) -> rusqlite::Result<FileRow> {
    Ok(FileRow {
        dir_id: r.get(0)?,
        name: names::decode(r.get(1)?),
        alloc_bytes: r.get::<_, i64>(2)? as u64,
        logical_bytes: r.get::<_, i64>(3)? as u64,
        // A kind added by a newer version reads as Other rather than failing.
        kind: Kind::from_index(r.get(4)?).unwrap_or_default(),
    })
}

fn scan_info_from_row(r: &Row<'_>) -> rusqlite::Result<ScanInfo> {
    Ok(ScanInfo {
        id: r.get(0)?,
        volume_id: r.get(1)?,
        root: PathBuf::from(names::decode(r.get(2)?)),
        started_at: r.get(3)?,
        finished_at: r.get(4)?,
        alloc_bytes: r.get::<_, i64>(5)? as u64,
        logical_bytes: r.get::<_, i64>(6)? as u64,
        file_count: r.get::<_, i64>(7)? as u64,
        dir_count: r.get::<_, i64>(8)? as u64,
        error_count: r.get::<_, i64>(9)? as u64,
        cancelled: r.get(10)?,
        elapsed_ms: r.get::<_, i64>(11)? as u64,
        has_tree: r.get(12)?,
    })
}

/// SQLite integers are signed 64-bit. No real size comes near the limit.
fn int(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn role_to_int(role: AppRole) -> u8 {
    match role {
        AppRole::Program => 0,
        AppRole::Data => 1,
        AppRole::Cache => 2,
    }
}

fn role_from_int(n: u8) -> AppRole {
    match n {
        0 => AppRole::Program,
        2 => AppRole::Cache,
        _ => AppRole::Data,
    }
}

fn kinds_to_blob(kinds: &[u64; KIND_COUNT]) -> Vec<u8> {
    kinds.iter().flat_map(|k| k.to_le_bytes()).collect()
}

/// Accepts blobs written when there were fewer kinds; missing ones are zero.
fn kinds_from_blob(blob: &[u8]) -> Option<[u64; KIND_COUNT]> {
    if !blob.len().is_multiple_of(8) {
        return None;
    }
    let mut kinds = [0u64; KIND_COUNT];
    for (slot, chunk) in kinds.iter_mut().zip(blob.chunks_exact(8)) {
        *slot = u64::from_le_bytes(chunk.try_into().ok()?);
    }
    Some(kinds)
}

fn flags_to_bits(f: DirFlags) -> u32 {
    (f.unreadable as u32)
        | (f.other_filesystem as u32) << 1
        | (f.excluded as u32) << 2
        | (f.cancelled as u32) << 3
}

fn flags_from_bits(bits: u32) -> DirFlags {
    DirFlags {
        unreadable: bits & 1 != 0,
        other_filesystem: bits & 2 != 0,
        excluded: bits & 4 != 0,
        cancelled: bits & 8 != 0,
    }
}

fn error_kind_to_int(kind: ScanErrorKind) -> u8 {
    match kind {
        ScanErrorKind::PermissionDenied => 0,
        ScanErrorKind::NotFound => 1,
        ScanErrorKind::Other => 2,
    }
}

fn error_kind_from_int(n: u8) -> ScanErrorKind {
    match n {
        0 => ScanErrorKind::PermissionDenied,
        1 => ScanErrorKind::NotFound,
        _ => ScanErrorKind::Other,
    }
}

/// Folder sizes worth remembering for Growth: shallow and not tiny.
fn snapshot_rows(tree: &ScanTree) -> Vec<(String, u32, u64)> {
    // Path of each folder that made it into the snapshot; children of a
    // folder that did not are skipped (they are deeper, and no larger).
    let mut kept: Vec<Option<(String, u32)>> = Vec::with_capacity(tree.dirs.len());
    let mut rows = Vec::new();
    for d in &tree.dirs {
        let entry = match d.parent {
            None => Some((String::new(), 0)),
            Some(p) => match &kept[p as usize] {
                Some((parent_path, depth))
                    if *depth < SNAPSHOT_MAX_DEPTH && d.alloc_bytes >= SNAPSHOT_MIN_BYTES =>
                {
                    let name = d.name.to_string_lossy();
                    let path = if parent_path.is_empty() {
                        name.into_owned()
                    } else {
                        format!("{parent_path}/{name}")
                    };
                    Some((path, depth + 1))
                }
                _ => None,
            },
        };
        if let Some((path, depth)) = &entry {
            rows.push((path.clone(), *depth, d.alloc_bytes));
        }
        kept.push(entry);
    }
    rows
}
