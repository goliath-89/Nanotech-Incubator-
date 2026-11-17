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
    path: String,
    volume_id: String,
    fast: bool,
) -> Reply<u64> {
    let token = state.next_token.fetch_add(1, Ordering::Relaxed);
    let ctl = Arc::new(ScanControl::new());
    state.scans.lock().map_err(text)?.insert(token, ctl.clone());
    let store = state.store.clone();

    std::thread::Builder::new()
        .name(format!("strata-scan-{token}"))
        .spawn(move || {
            let root = PathBuf::from(&path);
            let started_at = now();
            let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));

            // Progress ten times a second while the scan runs.
            let ticker = {
                let (app, ctl, finished) = (app.clone(), ctl.clone(), finished.clone());
                std::thread::spawn(move || {
                    while !finished.load(Ordering::Relaxed) {
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

            let result = scan_root(&root, fast, &ctl)
                .map_err(text)
                .and_then(|outcome| {
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
