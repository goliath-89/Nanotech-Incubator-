//! The commands and events between the window and the Rust engine.
//! See docs/ARCHITECTURE.md, "Bridge". Sizes cross as decimal strings
//! because JavaScript numbers lose precision above 2^53.

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use strata_core::{volumes, ScanControl, VolumeKind, KIND_COUNT};
use strata_pipeline::ring::{ring_tree, RingNode, Slice};
use strata_pipeline::{now, save_everything, scan_root, Method};
use strata_store::{AppRole, DirRow, FileRow, ScanMeta, Store};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    store: Arc<Mutex<Store>>,
    /// Strata's own data folder: protected from cleanup.
    data_folder: PathBuf,
    scans: Mutex<HashMap<u64, Arc<ScanControl>>>,
    next_token: AtomicU64,
}

impl AppState {
    /// Open (or create) the scan history in the app's data folder.
    pub fn open(app: &tauri::App) -> Result<AppState, Box<dyn std::error::Error>> {
        let folder = app.path().app_data_dir()?;
        std::fs::create_dir_all(&folder)?;
        let store = Store::open(&folder.join("history.sqlite"))?;
        Ok(AppState {
            store: Arc::new(Mutex::new(store)),
            data_folder: folder,
            scans: Mutex::new(HashMap::new()),
            next_token: AtomicU64::new(1),
        })
    }
}

/// Errors reach the window as plain sentences.
type Reply<T> = Result<T, String>;

fn text<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDto {
    id: String,
    name: String,
    mount: String,
    file_system: String,
    kind: &'static str,
    read_only: bool,
    total: String,
    used: String,
    free: String,
}

#[tauri::command]
pub fn list_volumes() -> Reply<Vec<VolumeDto>> {
    let list = volumes().map_err(text)?;
    Ok(list
        .into_iter()
        .map(|v| VolumeDto {
            kind: match v.kind {
                VolumeKind::Internal => "internal",
                VolumeKind::External => "external",
                VolumeKind::Network => "network",
            },
            total: v.usage.total_bytes.to_string(),
            used: v.usage.used_bytes().to_string(),
            free: v.usage.available_bytes.to_string(),
            mount: v.mount.to_string_lossy().into_owned(),
            id: v.id,
            name: v.name,
            file_system: v.file_system,
            read_only: v.read_only,
        })
        .collect())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    token: u64,
    files: String,
    dirs: String,
    bytes: String,
    errors: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PartialTop {
    name: String,
    bytes: String,
    main_kind: u8,
}

/// What has been counted so far in each folder directly under the root,
/// largest first: drives the ring filling in while the scan runs.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Partial {
    token: u64,
    bytes: String,
    top: Vec<PartialTop>,
}

fn partial(token: u64, ctl: &ScanControl) -> Option<Partial> {
    let mut top = ctl.top_level_so_far();
    if top.is_empty() {
        return None;
    }
    top.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    Some(Partial {
        token,
        bytes: ctl.bytes().to_string(),
        top: top
            .into_iter()
            .take(200)
            .map(|t| PartialTop {
                name: t.name.to_string_lossy().into_owned(),
                bytes: t.bytes.to_string(),
                main_kind: (0..KIND_COUNT)
                    .max_by_key(|&k| (t.kind_bytes[k], std::cmp::Reverse(k)))
                    .filter(|&k| t.kind_bytes[k] > 0)
                    .unwrap_or(0) as u8,
            })
            .collect(),
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    token: u64,
    scan_id: i64,
    /// "fileTable" or "walk".
    method: &'static str,
    /// Why the fast scan was not used, when it was wanted.
    fallback_reason: Option<String>,
    elapsed_ms: String,
    error_count: String,
    cancelled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Failed {
    token: u64,
    message: String,
}

/// Start scanning `path` in the background. Returns a token; progress,
/// completion and failure arrive as `scan://progress`, `scan://done` and
/// `scan://failed` events carrying that token.
#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    volume_id: String,
    fast: bool,
) -> Reply<u64> {
    // The window names a drive by its id; the path to scan comes from the
    // system's own list of drives, never from the window.
    let volume = volumes()
        .map_err(text)?
        .into_iter()
        .find(|v| v.id == volume_id)
        .ok_or("That drive is no longer connected")?;
    if volume.kind == VolumeKind::Network {
        return Err("Network drives are not scanned".into());
    }
    let path = volume.mount;
    let token = state.next_token.fetch_add(1, Ordering::Relaxed);
    let ctl = Arc::new(ScanControl::new());
    {
        let mut scans = state.scans.lock().map_err(text)?;
        if !scans.is_empty() {
            return Err("A scan is already running".into());
        }
        scans.insert(token, ctl.clone());
    }
    let store = state.store.clone();
    let data_folder = state.data_folder.clone();

    std::thread::Builder::new()
        .name(format!("strata-scan-{token}"))
        .spawn(move || {
            let root = path;
            let started_at = now();
            let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));

            // Progress ten times a second while the scan runs.
            let ticker = {
                let (app, ctl, finished) = (app.clone(), ctl.clone(), finished.clone());
                std::thread::spawn(move || {
                    let mut tick = 0u32;
                    while !finished.load(Ordering::Relaxed) {
                        // Once a second: the running totals for the ring.
                        if tick % 10 == 5 {
                            if let Some(p) = partial(token, &ctl) {
                                let _ = app.emit("scan://partial", p);
                            }
                        }
                        tick += 1;
                        let _ = app.emit(
                            "scan://progress",
                            Progress {
                                token,
                                files: ctl.files().to_string(),
                                dirs: ctl.dirs().to_string(),
                                bytes: ctl.bytes().to_string(),
                                errors: ctl.errors().to_string(),
                            },
                        );
                        std::thread::sleep(Duration::from_millis(100));
                    }
                })
            };

            let result =
                run_scan(&app, token, &root, fast, &ctl, &data_folder).and_then(|outcome| {
                    let meta = ScanMeta {
                        volume_id,
                        started_at,
                        finished_at: now(),
                    };
                    let mut store = store.lock().map_err(text)?;
                    let scan_id =
                        save_everything(&mut store, &outcome.tree, &meta).map_err(text)?;
                    Ok(Done {
                        token,
                        scan_id,
                        method: match outcome.method {
                            Method::FileTable => "fileTable",
                            Method::Walk => "walk",
                        },
                        fallback_reason: outcome.fallback_reason,
                        elapsed_ms: outcome.tree.stats.elapsed_ms.to_string(),
                        error_count: outcome.tree.stats.error_count.to_string(),
                        cancelled: outcome.tree.stats.cancelled,
                    })
                });
            finished.store(true, Ordering::Relaxed);
            let _ = ticker.join();
            if let Some(state) = app.try_state::<AppState>() {
                if let Ok(mut scans) = state.scans.lock() {
                    scans.remove(&token);
                }
            }
            match result {
                Ok(done) => {
                    let _ = app.emit("scan://done", done);
                }
                Err(message) => {
                    let _ = app.emit("scan://failed", Failed { token, message });
                }
            }
        })
        .map_err(text)?;
    Ok(token)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Stage {
    token: u64,
    /// "permission" (waiting for the Windows prompt), "fileTable" or "walk".
    stage: &'static str,
}

/// Choose and run the scanner. On Windows, a whole drive with the fast scan
/// asked for, and no administrator rights yet: ask Windows for them through
/// the elevated copy (see elevated.rs); if that is declined or fails, walk
/// the folders and say why.
fn run_scan(
    app: &AppHandle,
    token: u64,
    root: &std::path::Path,
    fast: bool,
    ctl: &ScanControl,
    data_folder: &std::path::Path,
) -> Result<strata_pipeline::Outcome, String> {
    let stage = |stage| {
        let _ = app.emit("scan://stage", Stage { token, stage });
    };
    #[cfg(windows)]
    if fast && !strata_mft::is_elevated() {
        if let Some(letter) = whole_drive(root) {
            stage("permission");
            match crate::elevated::scan_drive_elevated(letter, data_folder) {
                Ok(tree) => {
                    return Ok(strata_pipeline::Outcome {
                        tree,
                        method: Method::FileTable,
                        fallback_reason: None,
                    })
                }
                Err(e) => {
                    stage("walk");
                    let mut outcome = scan_root(root, false, ctl).map_err(text)?;
                    outcome.fallback_reason = Some(match e {
                        crate::elevated::ElevatedError::Declined => {
                            "Administrator access was not allowed, so folders were read one by one"
                                .to_string()
                        }
                        other => other.to_string(),
                    });
                    return Ok(outcome);
                }
            }
        }
    }
    let _ = data_folder;
    stage(if fast { "fileTable" } else { "walk" });
    scan_root(root, fast, ctl).map_err(text)
}

/// `C:\` and the like.
#[cfg(windows)]
fn whole_drive(root: &std::path::Path) -> Option<char> {
    let text = root.to_string_lossy();
    let mut chars = text.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else {
        return None;
    };
    (letter.is_ascii_alphabetic() && matches!(chars.as_str(), "" | "\\" | "/")).then_some(letter)
}

/// Ask a running scan to stop. It finishes with what it has counted.
#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>, token: u64) -> Reply<()> {
    if let Some(ctl) = state.scans.lock().map_err(text)?.get(&token) {
        ctl.cancel();
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerDto {
    app: String,
    role: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirDto {
    id: u32,
    name: String,
    alloc_bytes: String,
    file_count: String,
    dir_count: String,
    /// Bytes per kind, in `Kind` order (Other, Videos, Photos, ...).
    kind_bytes: Vec<String>,
    cloud_only_bytes: String,
    own_files_bytes: String,
    /// "unreadable", "otherVolume", "excluded", "cancelled" or null.
    not_scanned: Option<&'static str>,
    owner: Option<OwnerDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    name: String,
    alloc_bytes: String,
    kind: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    id: u32,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    dir: DirDto,
    path: String,
    breadcrumb: Vec<Crumb>,
    children: Vec<DirDto>,
    files: Vec<FileDto>,
}

fn dir_dto(d: DirRow, app_names: &[String]) -> DirDto {
    let not_scanned = if d.flags.unreadable {
        Some("unreadable")
    } else if d.flags.other_filesystem {
        Some("otherVolume")
    } else if d.flags.excluded {
        Some("excluded")
    } else if d.flags.cancelled {
        Some("cancelled")
    } else {
        None
    };
    debug_assert_eq!(d.kind_bytes.len(), KIND_COUNT);
    DirDto {
        id: d.id,
        name: d.name.to_string_lossy().into_owned(),
        alloc_bytes: d.alloc_bytes.to_string(),
        file_count: d.file_count.to_string(),
        dir_count: d.dir_count.to_string(),
        kind_bytes: d.kind_bytes.iter().map(u64::to_string).collect(),
        cloud_only_bytes: d.cloud_only_bytes.to_string(),
        own_files_bytes: d.own_files_alloc.to_string(),
        not_scanned,
        owner: d.owner.and_then(|o| {
            Some(OwnerDto {
                app: app_names.get(o.app as usize)?.clone(),
                role: match o.role {
                    AppRole::Program => "program",
                    AppRole::Data => "data",
                    AppRole::Cache => "cache",
                },
            })
        }),
    }
}

fn file_dto(f: FileRow) -> FileDto {
    FileDto {
        name: f.name.to_string_lossy().into_owned(),
        alloc_bytes: f.alloc_bytes.to_string(),
        kind: f.kind as u8,
    }
}

/// One folder of a saved scan: itself, the way to it, what is inside.
#[tauri::command]
pub fn get_dir(state: State<'_, AppState>, scan_id: i64, dir_id: u32) -> Reply<Listing> {
    let store = state.store.lock().map_err(text)?;
    let listing = store.children(scan_id, dir_id).map_err(text)?;
    let app_names: Vec<String> = store
        .apps(scan_id)
        .map_err(text)?
        .into_iter()
        .map(|a| a.name)
        .collect();
    let path: PathBuf = listing.breadcrumb.iter().map(|(_, n)| n).collect();
    Ok(Listing {
        path: path.to_string_lossy().into_owned(),
        breadcrumb: listing
            .breadcrumb
            .into_iter()
            .map(|(id, name)| Crumb {
                id,
                name: name.to_string_lossy().into_owned(),
            })
            .collect(),
        dir: dir_dto(listing.dir, &app_names),
        children: listing
            .children
            .into_iter()
            .map(|d| dir_dto(d, &app_names))
            .collect(),
        files: listing.files.into_iter().map(file_dto).collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    scan_id: i64,
    finished_at: i64,
    alloc_bytes: String,
    file_count: String,
}

/// The newest saved scan of a volume that still has its folders, if any.
#[tauri::command]
pub fn latest_scan(state: State<'_, AppState>, volume_id: String) -> Reply<Option<ScanSummary>> {
    let store = state.store.lock().map_err(text)?;
    let scans = store.scans(Some(&volume_id)).map_err(text)?;
    Ok(scans
        .into_iter()
        .find(|s| s.has_tree && !s.cancelled)
        .map(|s| ScanSummary {
            scan_id: s.id,
            finished_at: s.finished_at,
            alloc_bytes: s.alloc_bytes.to_string(),
            file_count: s.file_count.to_string(),
        }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RingDto {
    /// The folder to open on click; null for "files here" and "smaller folders".
    id: Option<u32>,
    /// "folder", "files" or "smaller".
    slice: &'static str,
    name: String,
    bytes: String,
    main_kind: u8,
    not_scanned: bool,
    children: Vec<RingDto>,
}

fn ring_dto(n: RingNode) -> RingDto {
    let (id, slice) = match n.slice {
        Slice::Folder(id) => (Some(id), "folder"),
        Slice::Files => (None, "files"),
        Slice::Smaller(_) => (None, "smaller"),
    };
    RingDto {
        id,
        slice,
        name: n.name,
        bytes: n.bytes.to_string(),
        main_kind: n.kind,
        not_scanned: n.not_scanned,
        children: n.children.into_iter().map(ring_dto).collect(),
    }
}

/// Three rings below a folder, for the ring view. Slices under 0.4% of
/// the centre (about 1.5 degrees) are folded together.
#[tauri::command]
pub fn get_ring(state: State<'_, AppState>, scan_id: i64, dir_id: u32) -> Reply<RingDto> {
    let store = state.store.lock().map_err(text)?;
    let ring = ring_tree(&store, scan_id, dir_id, 3, 0.004).map_err(text)?;
    Ok(ring_dto(ring))
}

// ---------------------------------------------------------------------------
// Apps, largest files, reveal (Day 13)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDto {
    /// Position in the list; pass it back to `get_app_dirs`.
    index: u32,
    name: String,
    kind: Option<u8>,
    app_bytes: String,
    data_bytes: String,
    cache_bytes: String,
    total_bytes: String,
}

/// Apps that own space in a scan, largest first.
#[tauri::command]
pub fn get_apps(state: State<'_, AppState>, scan_id: i64) -> Reply<Vec<AppDto>> {
    let store = state.store.lock().map_err(text)?;
    Ok(store
        .apps(scan_id)
        .map_err(text)?
        .into_iter()
        .enumerate()
        .map(|(i, a)| AppDto {
            index: i as u32,
            kind: a.kind.map(|k| k as u8),
            total_bytes: (a.app_bytes + a.data_bytes + a.cache_bytes).to_string(),
            app_bytes: a.app_bytes.to_string(),
            data_bytes: a.data_bytes.to_string(),
            cache_bytes: a.cache_bytes.to_string(),
            name: a.name,
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedDir {
    dir: DirDto,
    path: String,
}

/// Where an app's space is: its outermost folders, largest first.
#[tauri::command]
pub fn get_app_dirs(state: State<'_, AppState>, scan_id: i64, app: u32) -> Reply<Vec<PlacedDir>> {
    let store = state.store.lock().map_err(text)?;
    let names: Vec<String> = store
        .apps(scan_id)
        .map_err(text)?
        .into_iter()
        .map(|a| a.name)
        .collect();
    let dirs = store.app_dirs(scan_id, app, 40).map_err(text)?;
    dirs.into_iter()
        .map(|d| {
            let path = store.dir_path(scan_id, d.id).map_err(text)?;
            Ok(PlacedDir {
                path: path.to_string_lossy().into_owned(),
                dir: dir_dto(d, &names),
            })
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    dir_id: u32,
    name: String,
    folder: String,
    alloc_bytes: String,
    kind: u8,
}

/// The largest files of a scan, optionally of one kind.
#[tauri::command]
pub fn largest_files(
    state: State<'_, AppState>,
    scan_id: i64,
    kind: Option<u8>,
    limit: u32,
) -> Reply<Vec<LargeFile>> {
    let kind = match kind {
        Some(k) => Some(strata_core::Kind::from_index(k).ok_or("unknown kind")?),
        None => None,
    };
    let store = state.store.lock().map_err(text)?;
    let files = store
        .largest_files(scan_id, kind, limit.min(500) as usize)
        .map_err(text)?;
    let mut folders: HashMap<u32, String> = HashMap::new();
    files
        .into_iter()
        .map(|f| {
            let folder = match folders.get(&f.dir_id) {
                Some(p) => p.clone(),
                None => {
                    let p = store
                        .dir_path(scan_id, f.dir_id)
                        .map_err(text)?
                        .to_string_lossy()
                        .into_owned();
                    folders.insert(f.dir_id, p.clone());
                    p
                }
            };
            Ok(LargeFile {
                dir_id: f.dir_id,
                folder,
                alloc_bytes: f.alloc_bytes.to_string(),
                kind: f.kind as u8,
                name: f.name.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

/// Show a folder, or a file in it, in Explorer or Finder. The path is
/// rebuilt from the scan history; the window only names the folder by id
/// and the file by its bare name.
#[tauri::command]
pub fn reveal(
    state: State<'_, AppState>,
    scan_id: i64,
    dir_id: u32,
    file: Option<String>,
) -> Reply<()> {
    let mut path = {
        let store = state.store.lock().map_err(text)?;
        store.dir_path(scan_id, dir_id).map_err(text)?
    };
    if let Some(name) = file {
        // One plain name inside the folder: no separators, no drive or
        // stream prefix (a name like `C:x` would replace the whole path).
        let plain = std::path::Path::new(&name)
            .file_name()
            .is_some_and(|n| n == std::ffi::OsStr::new(&name));
        if !plain || name == "." || name == ".." || name.contains(['/', '\\', '\0', ':']) {
            return Err("not a file name".into());
        }
        path.push(name);
    }
    if !path.exists() {
        return Err(format!("{} is no longer there", path.display()));
    }
    open_in_file_manager(&path).map_err(|e| format!("could not open the file manager: {e}"))
}

#[cfg(windows)]
fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    // Arguments go straight to the program; no shell is involved.
    let mut select = std::ffi::OsString::from("/select,");
    select.push(path.as_os_str());
    std::process::Command::new("explorer.exe")
        .arg(select)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    std::process::Command::new("xdg-open")
        .arg(folder)
        .spawn()
        .map(|_| ())
}

// ---------------------------------------------------------------------------
// Reclaim (Day 14)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionDto {
    number: u32,
    group: String,
    contents_only: bool,
    path: String,
    bytes: String,
    app: Option<String>,
    rebuildable: bool,
    reason: String,
}

/// What a scan found to clean up, in display order.
#[tauri::command]
pub fn get_suggestions(state: State<'_, AppState>, scan_id: i64) -> Reply<Vec<SuggestionDto>> {
    let store = state.store.lock().map_err(text)?;
    Ok(store
        .numbered_suggestions(scan_id)
        .map_err(text)?
        .into_iter()
        .map(|(number, s)| SuggestionDto {
            number,
            group: s.group,
            contents_only: s.contents_only,
            path: s.path.to_string_lossy().into_owned(),
            bytes: s.bytes.to_string(),
            app: s.app,
            rebuildable: s.rebuildable,
            reason: s.reason,
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupDto {
    done: Vec<u32>,
    bytes_freed: String,
    refused: Vec<Problem>,
    failed: Vec<Problem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    path: String,
    reason: String,
}

/// Move the chosen suggestions to the Trash or Recycle Bin. The window
/// names suggestions by number only; paths come from the scan history and
/// every item is checked again by the safety engine before it moves.
#[tauri::command]
pub fn clean_up(state: State<'_, AppState>, scan_id: i64, numbers: Vec<u32>) -> Reply<CleanupDto> {
    let mut policy = strata_safety::Policy::for_current_user().map_err(|r| format!("{r:?}"))?;
    // Strata's own history must never be offered for cleanup; stop if it
    // cannot be protected.
    policy
        .protect(&state.data_folder.to_string_lossy())
        .map_err(|r| format!("could not protect Strata's own data folder: {r:?}"))?;
    let mut store = state.store.lock().map_err(text)?;
    let report =
        strata_pipeline::cleanup::clean_up(&mut store, scan_id, &numbers, &policy).map_err(text)?;
    let problems = |v: Vec<(PathBuf, String)>| {
        v.into_iter()
            .map(|(p, reason)| Problem {
                path: p.to_string_lossy().into_owned(),
                reason,
            })
            .collect()
    };
    Ok(CleanupDto {
        done: report.done,
        bytes_freed: report.bytes_freed.to_string(),
        refused: problems(report.refused),
        failed: problems(report.failed),
    })
}

/// Open the Recycle Bin or Trash, so the person can empty it (or restore
/// from it) themselves. Strata never empties it.
#[tauri::command]
pub fn open_trash() -> Reply<()> {
    #[cfg(windows)]
    let result = std::process::Command::new("explorer.exe")
        .arg("shell:RecycleBinFolder")
        .spawn();
    #[cfg(target_os = "macos")]
    let result = match std::env::var_os("HOME") {
        Some(home) => std::process::Command::new("open")
            .arg(PathBuf::from(home).join(".Trash"))
            .spawn(),
        None => return Err("no home folder".into()),
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg("trash:///")
        .spawn();
    let place = if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    };
    result
        .map(|_| ())
        .map_err(|e| format!("could not open the {place}: {e}"))
}
