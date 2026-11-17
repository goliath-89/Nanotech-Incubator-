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
| `crates/strata-safety` | Protected paths, move to Trash / Recycle Bin | Built |
| `crates/strata-reclaim` | Cleanup suggestions and duplicate finder (read-only) | Built |
| `crates/strata-mft` | Fast whole-volume scan from the NTFS file table | Built; first run on real Windows pending |
| `crates/strata-classify` | File kinds and app attribution from rules | Built |
| `rules/` | App attribution rules (TOML), 20 starter apps per system | Started; grows in Sprint 5 |
| `crates/strata-pipeline` | Picks the scanner, saves a scan with apps and suggestions (shared by app and CLI) | Built |
| `app/src-tauri` | Tauri commands and events (own workspace: needs webview libraries) | Built: volumes, scan, cancel, folders, latest scan |
| `app/src` | Svelte 5 views; `lib/api.ts` is the only file that calls Rust, and serves example data in a plain browser | Started |

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
- `Classify`: `folder_kind(&self, dir: &Path) -> Option<Kind>` is called once
  per folder; `Some` gives every file directly in it that kind. Otherwise
  `file_kind(&self, name: &OsStr) -> Kind` is called per file. Both run on
  many threads. `NoClassify` returns `Kind::Other`.
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

7. **Files that vanish mid-scan are not errors.** On a drive in use, a file
   listed a moment ago may be gone when examined; it is skipped silently.

`DirFlags::not_scanned()` is what the UI shows as "Not scanned".

### Volumes

```rust
volumes() -> io::Result<Vec<Volume>>     // internal first, then by mount point
volume_usage(path) -> io::Result<VolumeUsage>
```

`Volume`: `id` (UUID on macOS, serial number on Windows, device name on
Linux; this is the `volume_id` saved with scans), `name` as the system shows
it, `mount` (the folder to scan), `file_system`, `kind` (Internal, External,
Network), `read_only`, `usage`. Helper volumes the system hides (macOS
"don't browse" mounts, pseudo file systems) are left out. Network volumes
are listed but not offered for scanning.

### Known gaps, with the day they are closed

| Gap | Effect | Plan |
| --- | --- | --- |
| Windows hard links are not de-duplicated by the plain walk | `C:\Windows\WinSxS` over-counted by a few GB | The fast scan (`strata-mft`) counts them once. The plain walk still over-counts; Day 6 adds handle lookups under known link-heavy folders |
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
| `dirs` | Every folder of a scan, keyed `(scan_id, id)`, same fields as `DirNode`, plus its owning app and role |
| `files` | The largest files, keyed `(scan_id, rank)`, rank 0 = largest |
| `scan_errors` | What could not be read |
| `snapshots` | Folder sizes to depth 6 and at least 1 MiB, by relative path, for Growth |
| `scan_apps` | Per-app totals of a scan, largest first; `dirs.app_idx` / `dirs.app_role` point into it |
| `suggestions` | Cleanup suggestions as computed right after the scan, in display order |

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
save_apps(ScanId, &[AppRow], &[Option<DirOwner>]) / apps(ScanId)      // one owner entry per folder
save_suggestions(ScanId, &[SuggestionRow]) / suggestions(ScanId)
prune(volume_id, keep)                      // older scans keep totals, app totals + snapshot; lose folders and suggestions
delete(ScanId)
```

The store knows nothing about the classifier or the cleanup engine: it has
its own plain row types (`AppRow`, `DirOwner`, `SuggestionRow`) and the
caller converts. After every scan the app does what `strata scan --save`
does (`save_everything` in `crates/strata-cli/src/main.rs` is the reference):
save the tree, run `attribute` and save apps and owners, run `suggest` and
save the suggestions. `DirRow.owner` then tells a view which app a folder
belongs to. Saved suggestions describe the disk at scan time; `trash`
re-checks every item when the user acts.

The app calls `children` for each level it draws and never loads a whole
tree. `load` and `children` on a pruned scan return `StoreError::TreePruned`.

## Bridge (Tauri commands and events)

All sizes cross the bridge as strings of decimal digits (JavaScript numbers
lose precision above 2^53). Paths cross as an id pair `(scan_id, dir_id)` plus
a display string; the UI never builds a path itself.

| Command | Arguments | Returns |
| --- | --- | --- |
| `list_volumes` | none | `[{ id, name, mount, total, used, free, kind }]` |
| `start_scan` | `{ volume_id, fast }` (the path comes from `volumes()`, never from the window; one scan at a time) | `{ scan_token }` |
| `cancel_scan` | `{ scan_token }` | none |
| `get_dir` | `{ scan_id, dir_id }` | `{ dir, children[], files[], breadcrumb[] }` |
| `largest_files` | `{ scan_id, kind?, limit }` | `[{ dir_id, name, path, alloc, kind }]` |
| `by_app` | `{ scan_id }` | `[{ app, app_bytes, data_bytes, cache_bytes }]` |
| `suggestions` | `{ scan_id }` | grouped Reclaim items |
| `reveal` | `{ scan_id, dir_id, file? }` | none; path rebuilt from the history, file must be a bare name, no shell involved |
| `growth` | `{ volume_id, from_scan, to_scan }` | `[{ path, delta }]` |
| `get_apps` | `{ scan_id }` | apps largest first: `{ index, name, kind, appBytes, dataBytes, cacheBytes, totalBytes }` |
| `get_app_dirs` | `{ scan_id, app }` | an app's outermost folders with paths (`Store::app_dirs`) |
| `get_suggestions` | `{ scan_id }` | saved suggestions with their numbers |
| `clean_up` | `{ scan_id, numbers[] }` | `{ done[], bytesFreed, refused[], failed[] }`; paths come from the history, never from the window (`strata_pipeline::cleanup`) |
| `open_trash` | — | opens the Recycle Bin (Explorer) or Trash (Finder); never empties it |
| `preview_file` | `{ scan_id, dir_id, file }` | `{ kind: native \| image \| text \| cloud \| none, … }`: Quick Look on macOS; images and the first 64 KB of text elsewhere; never runs a file, follows a link or reads a cloud-only file |
| `list_exclusions` / `exclude_folder { scan_id, dir_id }` / `include_folder { path }` | | the list of folders left out of scans (`exclusions.json` in the data folder); applied to every scan method by `strata_pipeline::exclude` |
| `get_ring` | `{ scan_id, dir_id }` | three levels below the folder for the ring: `{ id, slice: folder/files/smaller, name, bytes, mainKind, notScanned, children }`, slices under 0.4% folded (`strata_pipeline::ring`) |

| Event | Payload | When |
| --- | --- | --- |
| `scan://stage` | `{ token, stage }`: `permission`, `fileTable` or `walk` | when the scan changes method; on Windows the app asks for administrator rights and runs a short-lived copy of itself (`--strata-elevated-scan`) that reads the file table and writes the result to a new file in the app's data folder, which the app reads and deletes. Declined → folder walk |
| `scan://progress` | `{ scan_token, files, dirs, bytes, errors }` | 10 times a second while scanning |
| `scan://partial` | `{ token, bytes, top[{ name, bytes, mainKind }] }`: totals so far per folder under the root, largest first (`ScanControl::top_level_so_far`, lock-free counters) | once a second during a folder walk, drives the ring filling in; the file-table scan has none (it takes seconds) |
| `scan://done` | `{ scan_token, scan_id, stats }` | scan finished or cancelled |
| `scan://failed` | `{ scan_token, message }` | root could not be read |

At start-up the bridge builds one `Policy::for_current_user()` and calls
`policy.protect()` on Strata's own data folder (the history database), so
nothing the app shows can remove its own records.

`list_volumes` is `strata_core::volumes()`. `start_scan` with `fast: true`
is `strata_mft::scan_volume`; on `NotNtfs` or `NeedsAdministrator` fall back
to the folder walk and say why.

`trash` is the only command that changes the user's files. It goes through
`strata-safety`, which re-checks every item against the protected list in
Rust; the UI's own filtering is a convenience, not the safeguard.

## strata-classify

**Kinds** (`KindClassifier`, the `Classify` the app scans with):

1. Where the folder is. Everything under a cache folder is Caches, under
   `node_modules` or `DerivedData` Developer, inside an `.app` or Program
   Files Apps, under `steamapps` Games, under a top-level system folder
   System. The first marker on the way down decides, with three exceptions:
   a game library inside a programs folder is Games, and caches or developer
   tools inside a system folder are Caches or Developer.
   Ordinary words people use for their own folders are treated with care:
   `Temp`, `tmp`, `cache` and `caches` are ignored inside Documents, Desktop,
   Downloads, Pictures, Movies, Music, Videos and OneDrive, and
   `Applications` / `Program Files` only count at the top of a drive or of a
   home folder.
2. Otherwise the file's name: a table of about 600 extensions.
3. Otherwise `Other`. The largest `Other` files are re-checked by file
   signature after the scan (Day 43); reading every unknown file's first
   bytes during the walk would slow it.

Kind numbers are stored in saved scans: never reorder `strata_core::Kind`.

**Apps** (`attribute(&ScanTree, &RuleSet, &Env) -> Attribution`) runs after
the scan, on folders. A folder belongs to the app whose rule names it or the
nearest folder above it; a deeper rule wins, so an app's cache folder inside
its data folder is cache. Each folder directly inside an applications folder
that no rule names is an app called by its own name. Everything else has no
app; nothing is guessed. `Attribution` gives per-app `app / data / cache`
bytes (largest first), the owner of every folder, and `cache_dirs`, the
folders a cache rule named (the candidates for Reclaim).

Rules are TOML, one file per system, compiled in and replaceable by an update:

```toml
[[app]]
name  = "Docker"
id    = "com.docker.docker"
kind  = "Developer"                                   # optional
app   = ["/Applications/Docker.app"]                  # the program
data  = ["~/Library/Containers/com.docker.docker"]    # what it created
cache = ["~/Library/Caches/com.docker.docker"]        # what it rebuilds
```

`~` is the home folder, `C:` the Windows system drive, `*` any one folder
name, `Name*` and `*.ext` match by start or end. A rules file with any
mistake (unknown key, path too broad, `..`, duplicate id) is rejected whole.
A `cache` path must already be removable under `strata-safety`; the test
`every_cache_rule_is_inside_a_place_the_safety_policy_allows` enforces it.

## strata-safety

The only code in Strata that removes anything, and it can only move things to
the Trash or Recycle Bin.

```rust
Policy::for_current_user() / Policy::new(platform, extra_homes)
policy.check(path) -> Result<(), Refusal>      // text only, no file access
policy.protect(folder)                         // add a protected tree; there is no way to remove one
vet(policy, scan_root, item) -> Result<PathBuf, VetError>   // every check, without moving
trash(policy, scan_root, items) -> TrashReport // { moved, refused, failed }
```

- **The policy is three lists in `src/rules.rs`** (macOS, Windows, Linux),
  each entry `Tree` (the path and everything in it), `Exact` (the path
  itself) or `Open` (a hole in a tree: a cache folder whose contents may go).
  The longest matching rule decides. Anything that contains a listed path is
  refused too. On macOS, programs (`.app`, `.framework`, ...) and the insides
  of library packages (`.photoslibrary`, ...) are refused wherever they are.
- **Open rules exist only in that file.** App rules (`rules/*.yaml`) cannot
  open anything: a rule that marks a path `rebuildable` must already pass
  `policy.check`, and a test in `strata-classify` enforces that for every rule.
- **Strict parsing.** Relative paths, `..`, network paths, Windows stream
  names, trailing dots and spaces, short-name aliases (`PROGRA~1`) and
  device names (`NUL`, `CON`, `COM1`) are refused, not tidied. macOS and Windows compare without regard to case,
  including the non-ASCII letters those systems treat as ASCII ones.
- **`vet` re-checks at the real location.** After the text check it resolves
  links in the folders above the item and checks again, so a linked folder
  cannot lead into a protected area or out of the scanned folder. Links and
  other drives' mount points are refused outright.
- The bridge's `trash` command must call `trash()` and nothing else.

## strata-reclaim

Read-only. Decides what to offer; `strata-safety` does the moving.

```rust
suggest(&ScanTree, &Attribution, &Env, &Policy, &Options) -> Vec<Suggestion>
expand(&Target) -> io::Result<Vec<PathBuf>>      // paths to hand to strata_safety::trash, listed at that moment
find_duplicates(&ScanTree, &Policy, &DuplicateControl) -> Vec<DuplicateGroup>
```

- `Suggestion`: `group` (AppCache, DeveloperJunk, Cache, OldInstaller,
  OldDownload), `target` (`Contents(folder)`: empty it, keep it; or
  `Item(path)`), `bytes`, `app`, `rebuildable`, and a one-sentence `reason`.
- Sources, in order of precedence: cache folders named by app rules; tool
  caches at fixed places (`~/.gradle/caches`, `~/.cargo/registry`, ...);
  rebuildable project folders by name, some only next to a marker file
  (`target` beside `Cargo.toml`); other children of the system cache and log
  folders; large files in Downloads past an age (installers 14 days, others 90).
- No two suggestions overlap, so their bytes add up to what would be freed.
  Anything the policy refuses is left out. Below 10 MiB is left out.
- Duplicates: only personal files (Videos, Photos, Music, Documents, Other)
  among the scan's large files, and only files that take space on this disk
  (reading a cloud placeholder would make the system download it). Grouped by length, then a hash of both ends,
  then a hash of everything. A copy the policy protects is listed first as
  the one to keep and is never offered. Files changed since the scan are
  dropped. A cancelled search returns nothing.

## strata-mft

Reads the NTFS Master File Table front to back instead of walking folders.
Read-only, and generic over `Read + Seek`, so it runs against volume images
on any system and against `\\.\C:` on Windows (administrator needed).

```rust
scan_volume(volume, &MftOptions, &classify, &ScanControl) -> Result<ScanTree, MftError>
is_ntfs(&mut volume) -> bool
open_drive('C') -> Result<File, MftError>      // Windows only
```

- The result is an ordinary `ScanTree`, built through `strata_core::raw`
  (`RawDir`, `build_tree`), the same step the folder walk uses, so both
  scanners apply the same hard-link and roll-up rules.
- Space on disk is added up from each stream's run list: holes are left
  out, so sparse and compressed files count what they occupy, contents
  stored inside the file record count as zero, and NTFS's bad-cluster map
  (whose stream spans the volume) counts as nothing. Named streams and a
  folder's own index count too. On the test images the total equals the
  volume's own used-space figure to the byte.
- NTFS's own files (`$MFT`, `$LogFile`, `$Extend\...`) appear under the root
  as kind System. Short 8.3 alias names are ignored. A name whose parent
  folder was deleted or reused is dropped.
- All input is untrusted. Every offset is bounds-checked; a damaged record
  is skipped; a damaged table is an error. A test flips bytes across an
  image 400 times and requires no panic.
- Errors: `NotNtfs` (fall back to the walk), `NeedsAdministrator` (not elevated),
  `AccessRefused` (elevated but refused, usually antivirus), `Damaged`,
  `Cancelled`, `Io`.
- Tests use `tests/fixtures/small.img.gz`, a real NTFS volume built by
  `tools/make-ntfs-fixture.sh` (ntfs-3g), and compare against the listing
  the mounted volume gave.
