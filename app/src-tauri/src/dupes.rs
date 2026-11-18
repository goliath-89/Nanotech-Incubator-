//! Duplicate files: find identical copies among a saved scan's large
//! personal files, and move the copies the person picks to the Trash.
//!
//! The window names copies by (group, copy) number only. Paths come from
//! the search kept here, every move goes through the safety engine, and at
//! least one copy of every group always stays.

use crate::bridge::{dates, open_in_file_manager, text, AppState, Problem, Reply};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use strata_reclaim::{DuplicateCandidate, DuplicateControl, DuplicateGroup};
use tauri::State;

/// Files the search looks at: the largest a scan kept (1 MB or more).
const MAX_FILES: usize = 50_000;

pub(crate) struct Run {
    scan_id: i64,
    ctl: Arc<DuplicateControl>,
    /// Bytes the search may have to read, for progress.
    total_bytes: u64,
    files_checked: u64,
    groups: Option<Vec<DuplicateGroup>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyDto {
    name: String,
    folder: String,
    /// False when the safety rules protect this copy (inside an app or a
    /// photo library): it can never be the one removed.
    removable: bool,
    created: Option<i64>,
    modified: Option<i64>,
    /// When it was last opened, where the system records it.
    accessed: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDto {
    /// Bytes on disk of one copy.
    bytes: String,
    copies: Vec<CopyDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupesDto {
    groups: Vec<GroupDto>,
    /// Files compared (personal files of 1 MB or more).
    files_checked: String,
    cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeProgress {
    bytes_read: String,
    total_bytes: String,
}

fn accessed(path: &Path) -> Option<i64> {
    let t = std::fs::symlink_metadata(path).ok()?.accessed().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn dto(groups: &[DuplicateGroup], files_checked: u64, cancelled: bool) -> DupesDto {
    DupesDto {
        groups: groups
            .iter()
            .map(|g| GroupDto {
                bytes: g.alloc_bytes.to_string(),
                copies: g
                    .files
                    .iter()
                    .map(|f| {
                        let (created, modified) = dates(&f.path);
                        CopyDto {
                            name: f
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            folder: f
                                .path
                                .parent()
                                .map(|p| p.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            removable: f.removable,
                            created,
                            modified,
                            accessed: accessed(&f.path),
                        }
                    })
                    .collect(),
            })
            .collect(),
        files_checked: files_checked.to_string(),
        cancelled,
    }
}

fn policy(state: &AppState) -> Reply<strata_safety::Policy> {
    let mut policy = strata_safety::Policy::for_current_user().map_err(|r| format!("{r:?}"))?;
    policy
        .protect(&state.data_folder.to_string_lossy())
        .map_err(|r| format!("could not protect Strata's own data folder: {r:?}"))?;
    Ok(policy)
}

/// Look for identical files among a scan's large personal files. Reads the
/// files (a quick sample first, the whole file only when samples match), so
/// it can take a while; `duplicate_progress` reports how far it is.
#[tauri::command]
pub async fn find_duplicates(state: State<'_, AppState>, scan_id: i64) -> Reply<DupesDto> {
    // The scan's files, with their folders' paths (each folder looked up once).
    let candidates: Vec<DuplicateCandidate> = {
        let store = state.store.lock().map_err(text)?;
        let files = store
            .largest_files(scan_id, None, MAX_FILES)
            .map_err(text)?;
        let mut folders: HashMap<u32, PathBuf> = HashMap::new();
        let mut out = Vec::with_capacity(files.len());
        for f in files {
            let folder = match folders.get(&f.dir_id) {
                Some(p) => p.clone(),
                None => {
                    let p = store.dir_path(scan_id, f.dir_id).map_err(text)?;
                    folders.insert(f.dir_id, p.clone());
                    p
                }
            };
            out.push(DuplicateCandidate {
                path: folder.join(&f.name),
                logical_bytes: f.logical_bytes,
                alloc_bytes: f.alloc_bytes,
                kind: f.kind,
            });
        }
        out
    };
    // What may have to be read: files sharing their length with another.
    let mut lengths: HashMap<u64, u64> = HashMap::new();
    for c in &candidates {
        *lengths.entry(c.logical_bytes).or_default() += 1;
    }
    let total_bytes = candidates
        .iter()
        .filter(|c| lengths[&c.logical_bytes] > 1)
        .map(|c| c.logical_bytes)
        .sum();
    let files_checked = candidates.len() as u64;
    let ctl = Arc::new(DuplicateControl::new());
    {
        let mut run = state.dupes.lock().map_err(text)?;
        if let Some(old) = run.as_ref() {
            old.ctl.cancel(); // one search at a time
        }
        *run = Some(Run {
            scan_id,
            ctl: ctl.clone(),
            total_bytes,
            files_checked,
            groups: None,
        });
    }
    let policy = policy(&state)?;
    let worker = ctl.clone();
    let groups = tauri::async_runtime::spawn_blocking(move || {
        strata_reclaim::find_duplicates_among(candidates, &policy, &worker)
    })
    .await
    .map_err(text)?;
    let cancelled = ctl.is_cancelled();
    let reply = dto(&groups, files_checked, cancelled);
    let mut run = state.dupes.lock().map_err(text)?;
    if let Some(r) = run.as_mut().filter(|r| Arc::ptr_eq(&r.ctl, &ctl)) {
        if !cancelled {
            r.groups = Some(groups);
        }
    }
    Ok(reply)
}

#[tauri::command]
pub fn duplicate_progress(state: State<'_, AppState>) -> Reply<DupeProgress> {
    let run = state.dupes.lock().map_err(text)?;
    Ok(match run.as_ref() {
        Some(r) => DupeProgress {
            bytes_read: r.ctl.bytes_read().to_string(),
            total_bytes: r.total_bytes.to_string(),
        },
        None => DupeProgress {
            bytes_read: "0".into(),
            total_bytes: "0".into(),
        },
    })
}

#[tauri::command]
pub fn cancel_duplicates(state: State<'_, AppState>) -> Reply<()> {
    if let Some(r) = state.dupes.lock().map_err(text)?.as_ref() {
        r.ctl.cancel();
    }
    Ok(())
}

/// The finished search for this scan, if there is one (dates read now).
#[tauri::command]
pub fn last_duplicates(state: State<'_, AppState>, scan_id: i64) -> Reply<Option<DupesDto>> {
    let run = state.dupes.lock().map_err(text)?;
    Ok(run
        .as_ref()
        .filter(|r| r.scan_id == scan_id)
        .and_then(|r| r.groups.as_ref().map(|g| dto(g, r.files_checked, false))))
}

fn copy_path(state: &AppState, scan_id: i64, group: u32, copy: u32) -> Reply<PathBuf> {
    let run = state.dupes.lock().map_err(text)?;
    run.as_ref()
        .filter(|r| r.scan_id == scan_id)
        .and_then(|r| r.groups.as_ref())
        .and_then(|g| g.get(group as usize))
        .and_then(|g| g.files.get(copy as usize))
        .map(|f| f.path.clone())
        .ok_or_else(|| "that copy is gone; look for duplicates again".to_string())
}

#[tauri::command]
pub fn reveal_duplicate(
    state: State<'_, AppState>,
    scan_id: i64,
    group: u32,
    copy: u32,
) -> Reply<()> {
    let path = copy_path(&state, scan_id, group, copy)?;
    if !path.exists() {
        return Err(format!("{} is no longer there", path.display()));
    }
    open_in_file_manager(&path).map_err(|e| format!("could not open the file manager: {e}"))
}

#[tauri::command]
pub fn preview_duplicate(
    state: State<'_, AppState>,
    scan_id: i64,
    group: u32,
    copy: u32,
) -> Reply<crate::preview::Preview> {
    let path = copy_path(&state, scan_id, group, copy)?;
    crate::preview::preview_at(&path)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeCleanup {
    /// (group, copy) pairs now in the Trash.
    moved: Vec<[u32; 2]>,
    bytes_freed: String,
    problems: Vec<Problem>,
}

/// Move the picked copies to the Trash. Refuses a pick that would leave a
/// group without a copy, and any copy the safety rules protect.
#[tauri::command]
pub fn clean_duplicates(
    state: State<'_, AppState>,
    scan_id: i64,
    picks: Vec<[u32; 2]>,
) -> Reply<DupeCleanup> {
    let policy = policy(&state)?;
    let mut store = state.store.lock().map_err(text)?;
    let info = store.scan_info(scan_id).map_err(text)?;
    let mut run = state.dupes.lock().map_err(text)?;
    let groups = run
        .as_mut()
        .filter(|r| r.scan_id == scan_id)
        .and_then(|r| r.groups.as_mut())
        .ok_or("look for duplicates again first")?;

    let mut problems = Vec::new();
    let mut accepted: Vec<(u32, u32, PathBuf, u64)> = Vec::new();
    let mut by_group: HashMap<u32, Vec<u32>> = HashMap::new();
    for [g, c] in picks {
        by_group.entry(g).or_default().push(c);
    }
    for (g, mut copies) in by_group {
        copies.sort_unstable();
        copies.dedup();
        let Some(group) = groups.get(g as usize) else {
            continue;
        };
        // At least one copy always stays.
        if copies.len() >= group.files.len() {
            problems.push(Problem {
                path: group.files[0].path.to_string_lossy().into_owned(),
                reason: "Every copy was picked; one must stay. Nothing in this group was moved."
                    .into(),
            });
            continue;
        }
        for c in copies {
            let Some(f) = group.files.get(c as usize) else {
                continue;
            };
            if !f.removable {
                problems.push(Problem {
                    path: f.path.to_string_lossy().into_owned(),
                    reason: "Protected: this copy is inside an app or a library.".into(),
                });
                continue;
            }
            accepted.push((g, c, f.path.clone(), group.alloc_bytes));
        }
    }

    let items: Vec<PathBuf> = accepted.iter().map(|a| a.2.clone()).collect();
    let report = strata_safety::trash(&policy, &info.root, &items);
    for (p, r) in report.refused {
        problems.push(Problem {
            path: p.to_string_lossy().into_owned(),
            reason: r.to_string(),
        });
    }
    for (p, e) in report.failed {
        problems.push(Problem {
            path: p.to_string_lossy().into_owned(),
            reason: e,
        });
    }
    let moved: Vec<&(u32, u32, PathBuf, u64)> = accepted
        .iter()
        .filter(|a| report.moved.contains(&a.2))
        .collect();
    let bytes: u64 = moved.iter().map(|a| a.3).sum();
    if !moved.is_empty() {
        // The cleanup itself happened; failing to record it is not an error.
        let _ = store.record_cleanup(
            strata_pipeline::now(),
            &info.volume_id,
            bytes,
            moved.len() as u64,
            &[("duplicate".into(), bytes)],
        );
    }
    let reply = DupeCleanup {
        moved: moved.iter().map(|a| [a.0, a.1]).collect(),
        bytes_freed: bytes.to_string(),
        problems,
    };
    // Forget moved copies, and groups left with a single copy. Numbers in
    // the window are refreshed from `last_duplicates` afterwards.
    for (gi, group) in groups.iter_mut().enumerate() {
        let gone: Vec<usize> = reply
            .moved
            .iter()
            .filter(|m| m[0] as usize == gi)
            .map(|m| m[1] as usize)
            .collect();
        let mut i = 0;
        group.files.retain(|_| {
            let keep = !gone.contains(&i);
            i += 1;
            keep
        });
    }
    groups.retain(|g| g.files.len() > 1);
    Ok(reply)
}
