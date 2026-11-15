use rusqlite::Connection;

/// Bump when the tables change, and add a step to `migrate`.
pub(crate) const SCHEMA_VERSION: i64 = 1;

const V1: &str = "
CREATE TABLE scans (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    volume_id     TEXT    NOT NULL,
    root          BLOB    NOT NULL,
    started_at    INTEGER NOT NULL,
    finished_at   INTEGER NOT NULL,
    alloc_bytes   INTEGER NOT NULL,
    logical_bytes INTEGER NOT NULL,
    file_count    INTEGER NOT NULL,
    dir_count     INTEGER NOT NULL,
    error_count   INTEGER NOT NULL,
    cancelled     INTEGER NOT NULL,
    elapsed_ms    INTEGER NOT NULL,
    hardlink_duplicates  INTEGER NOT NULL,
    hardlink_saved_bytes INTEGER NOT NULL,
    -- 0 once the folder and file rows have been pruned to save space
    has_tree      INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX scans_by_volume ON scans (volume_id, finished_at);

CREATE TABLE dirs (
    scan_id          INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    id               INTEGER NOT NULL,
    parent_id        INTEGER,
    name             BLOB    NOT NULL,
    alloc_bytes      INTEGER NOT NULL,
    logical_bytes    INTEGER NOT NULL,
    file_count       INTEGER NOT NULL,
    dir_count        INTEGER NOT NULL,
    kind_bytes       BLOB    NOT NULL,
    cloud_only_bytes INTEGER NOT NULL,
    own_files_alloc  INTEGER NOT NULL,
    own_file_count   INTEGER NOT NULL,
    flags            INTEGER NOT NULL,
    app_id           INTEGER,
    PRIMARY KEY (scan_id, id)
) WITHOUT ROWID;
CREATE INDEX dirs_by_parent ON dirs (scan_id, parent_id);

CREATE TABLE files (
    scan_id       INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    rank          INTEGER NOT NULL,          -- 0 = largest
    dir_id        INTEGER NOT NULL,
    name          BLOB    NOT NULL,
    alloc_bytes   INTEGER NOT NULL,
    logical_bytes INTEGER NOT NULL,
    kind          INTEGER NOT NULL,
    app_id        INTEGER,
    PRIMARY KEY (scan_id, rank)
) WITHOUT ROWID;
CREATE INDEX files_by_dir  ON files (scan_id, dir_id);
CREATE INDEX files_by_kind ON files (scan_id, kind, rank);

CREATE TABLE scan_errors (
    scan_id INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    seq     INTEGER NOT NULL,
    path    BLOB    NOT NULL,
    kind    INTEGER NOT NULL,
    message TEXT    NOT NULL,
    PRIMARY KEY (scan_id, seq)
) WITHOUT ROWID;

-- Compact, long-lived record of folder sizes for the Growth view. Kept after
-- the full tree of an old scan has been pruned.
CREATE TABLE snapshots (
    scan_id     INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,            -- relative to the scan root, '/' separated
    depth       INTEGER NOT NULL,
    alloc_bytes INTEGER NOT NULL
);
CREATE INDEX snapshots_by_path ON snapshots (scan_id, path);

CREATE TABLE apps (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    bundle_id TEXT,
    platform  TEXT NOT NULL
);

CREATE TABLE suggestions (
    id          INTEGER PRIMARY KEY,
    scan_id     INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    dir_id      INTEGER NOT NULL,
    file_name   BLOB,
    type        TEXT    NOT NULL,
    reason      TEXT    NOT NULL,
    rebuildable INTEGER NOT NULL,
    bytes       INTEGER NOT NULL
);
CREATE INDEX suggestions_by_scan ON suggestions (scan_id);
";

pub(crate) fn migrate(conn: &mut Connection) -> rusqlite::Result<i64> {
    let found: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if found < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(V1)?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
    }
    Ok(found)
}
