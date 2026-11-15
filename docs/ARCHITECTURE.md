# Strata architecture

This is the contract between the parts of Strata. Code that crosses a boundary
described here must match it; change this file in the same commit when a
contract changes.

## Parts

| Crate / folder | Job | Status |
| --- | --- | --- |
| `crates/strata-core` | Walk a folder tree and total what is on disk | Built |
| `crates/strata-cli` | `strata scan` command for testing and benchmarks | Built |
| `crates/strata-store` | Save scans and snapshots in SQLite | Built |
| `crates/strata-safety` | Protected paths, move to Trash / Recycle Bin | Day 4 |
| `crates/strata-mft` | Windows fast scan from the NTFS Master File Table | Days 5–6 |
| `crates/strata-classify` | File kinds and app attribution from rules | Day 7 |
| `rules/` | App attribution rules (YAML) | Day 7 onwards |
| `app/src-tauri` | Tauri commands and events | Day 9 |
| `app/ui` | Svelte views | Sprint 2 onwards |

Rule: only Rust crates touch the file system. The UI receives finished
numbers through the bridge and never reads, moves or deletes a file itself.

## strata-core

```rust
pub fn scan(opts: &ScanOptions, classify: &dyn Classify, ctl: &ScanControl)
    -> io::Result<ScanTree>
```

- `ScanOptions`: `root`, `one_file_system` (default true), `min_file_bytes`
  (default 1 MiB), `max_files` (default 50,000), `threads` (0 = one per core),
  `excludes` (exact folder paths).
- `ScanControl`: live counters `files()`, `dirs()`, `bytes()`, `errors()` and
  `cancel()`. The caller polls it from another thread (the app does so 10
  times a second) while `scan` blocks.
- `Classify`: `fn file_kind(&self, dir: &Path, name: &OsStr) -> Kind`, called
  from many threads, once per file. `NoClassify` returns `Kind::Other`.
- `ScanTree`: `dirs` (every folder, parent before child, index 0 = root,
  children sorted by name), `files` (the largest files, biggest first),
  `errors` (capped at 10,000; the exact count is `stats.error_count`), `stats`.
- `DirNode` totals cover the folder and everything below: `alloc_bytes`,
  `logical_bytes`, `file_count`, `dir_count`, `kind_bytes[Kind]`,
  `cloud_only_bytes`; plus `own_files_alloc` / `own_file_count` for what sits
  directly in the folder, and `flags`.

### Counting rules

1. **Size is space reserved on disk**, not file length. Unix: `st_blocks × 512`.
   Windows: length rounded up to the cluster size; compressed and sparse files
   are asked for exactly (`GetCompressedFileSizeW`); files of 512 bytes or
   less count as zero because NTFS stores them inside the file record.
2. **Folders count their own blocks** on Unix, as `du` does.
3. **A hard-linked file is counted once**, in the first folder in tree order
   (then by name) that links to it. Every link still counts as one entry in
   `file_count`. The choice is the same on every run.
4. **Links and mount points are never followed.** Symbolic links, junctions
   and folders on another file system are listed but not entered
   (`flags.other_filesystem`).
5. **Cloud-only files take zero bytes.** Their length is summed separately in
   `cloud_only_bytes` (macOS `SF_DATALESS`; Windows offline / recall attributes).
6. **Nothing is dropped silently.** A folder that cannot be listed gets
   `flags.unreadable` and an entry in `errors`.

`DirFlags::not_scanned()` is what the UI shows as "Not scanned".

### Known gaps, with the day they are closed

| Gap | Effect | Plan |
| --- | --- | --- |
| Windows hard links are not de-duplicated by the plain walk | `C:\Windows\WinSxS` over-counted by a few GB | Day 5–6: the MFT scanner sees link counts for free; the plain walk opens handles only under known link-heavy folders |
| APFS clones (copies that share blocks) count once per copy | Over-count on Macs with many duplicated files | Day 3: measure on a real Mac; report the gap to the OS figure as "Shared or purgeable" |
| macOS root scan and the Data volume | `/` and `/System/Volumes/Data` are separate volumes joined by firmlinks | Day 3: scanning "Macintosh HD" means scanning the Data volume and adding the sealed system volume as one System item |
| APFS local snapshots and purgeable space | Used space the walk cannot see | Day 3: read sizes from `tmutil` / volume attributes and show as their own items |
| Restore points and shadow copies on Windows | Same | Day 6: from volume information, shown under System |

## strata-store (SQLite)

One database file per user, in the app's data folder. A scan is saved in one
transaction: after a crash the file holds the whole scan or none of it. The
format version is in `PRAGMA user_version`; a file written by a newer version
is refused, never modified.

| Table | Holds |
| --- | --- |
| `scans` | One row per scan: volume, root, times, totals, `has_tree` |
| `dirs` | Every folder of a scan, keyed `(scan_id, id)`, same fields as `DirNode`, plus `app_id` |
| `files` | The largest files, keyed `(scan_id, rank)`, rank 0 = largest, plus `app_id` |
| `scan_errors` | What could not be read |
| `snapshots` | Folder sizes to depth 6 and at least 1 MiB, by relative path, for Growth |
| `apps`, `suggestions` | Empty until the classifier (Day 7) and Reclaim (Day 9) fill them |

Names and paths are stored as the exact bytes the OS gave (UTF-16 LE on
Windows), never as lossy text. Snapshot paths are display text.

```rust
Store::open(path) / Store::open_in_memory()
save(&ScanTree, &ScanMeta) -> ScanId        // ScanMeta: volume_id, started_at, finished_at
load(ScanId) -> ScanTree                    // identical to what was saved
scans(Option<volume_id>) / latest(volume_id) / scan_info(ScanId)
children(ScanId, dir) -> DirListing         // dir, breadcrumb, child folders and large files, largest first
dir_path(ScanId, dir) / largest_files(ScanId, Option<Kind>, limit) / errors(ScanId)
diff(before, after, limit) -> Vec<GrowthRow> // biggest change first; a folder and its parents each get a row
prune(volume_id, keep)                      // older scans keep totals + snapshot, lose folders (has_tree = 0)
delete(ScanId)
```

The app calls `children` for each level it draws and never loads a whole
tree. `load` and `children` on a pruned scan return `StoreError::TreePruned`.

## Bridge (Tauri commands and events)

All sizes cross the bridge as strings of decimal digits (JavaScript numbers
lose precision above 2^53). Paths cross as an id pair `(scan_id, dir_id)` plus
a display string; the UI never builds a path itself.

| Command | Arguments | Returns |
| --- | --- | --- |
| `list_volumes` | none | `[{ id, name, mount, total, used, free, kind }]` |
| `start_scan` | `{ volume_id \| path, fast }` | `{ scan_token }` |
| `cancel_scan` | `{ scan_token }` | none |
| `get_dir` | `{ scan_id, dir_id }` | `{ dir, children[], files[], breadcrumb[] }` |
| `largest_files` | `{ scan_id, kind?, limit }` | `[{ dir_id, name, path, alloc, kind }]` |
| `by_app` | `{ scan_id }` | `[{ app, app_bytes, data_bytes, cache_bytes }]` |
| `suggestions` | `{ scan_id }` | grouped Reclaim items |
| `reveal` | `{ scan_id, dir_id, file? }` | none |
| `trash` | `{ scan_id, items[] }` | `{ moved[], refused[{ item, reason }] }` |
| `growth` | `{ volume_id, from_scan, to_scan }` | `[{ path, delta }]` |

| Event | Payload | When |
| --- | --- | --- |
| `scan://progress` | `{ scan_token, files, dirs, bytes, errors }` | 10 times a second while scanning |
| `scan://partial` | `{ scan_token, top[] }` (top-level totals so far) | once a second, drives the ring filling in |
| `scan://done` | `{ scan_token, scan_id, stats }` | scan finished or cancelled |
| `scan://failed` | `{ scan_token, message }` | root could not be read |

`trash` is the only command that changes the user's files. It goes through
`strata-safety`, which re-checks every item against the protected list in
Rust; the UI's own filtering is a convenience, not the safeguard.

## strata-classify

- Kinds: `Other, Videos, Photos, Music, Documents, Apps, Games, Developer,
  System, Caches` (`strata_core::Kind`; numbers are stored, never reorder).
- Kind of a file: extension table first; files with no known extension stay
  `Other` during the scan and the largest are re-checked by file signature
  afterwards (reading every unknown file's first bytes would slow the walk).
- App attribution runs after the scan, on folders, longest matching rule first.

```yaml
# rules/macos/docker.yaml
app: Docker
bundle_id: com.docker.docker
paths:
  - { path: "/Applications/Docker.app",                       role: app }
  - { path: "~/Library/Containers/com.docker.docker",         role: data }
  - { path: "~/Library/Caches/com.docker.docker",             role: cache, rebuildable: true }
```

`role` is `app`, `data` or `cache`. `~` and `%NAME%` expand per user.
A folder with no matching rule has no app; nothing is guessed.

## strata-safety

- `is_protected(path) -> Option<Reason>`: operating-system folders, the
  running app, installed app bundles and program folders, user-profile roots,
  anything outside the scanned volume, and every ancestor of a protected path.
- `trash(paths) -> Report`: refuses protected paths, then moves the rest to
  the Trash / Recycle Bin. There is no code path that deletes outright.
- Suggestions may only name items that match an allow rule (a known cache,
  installer or rebuildable folder) and are not protected.
