//! The steps around a scan, shared by the app and the `strata` command so
//! both behave the same: choose the scanner, run it, then save the result
//! together with app attribution and cleanup suggestions.

#![forbid(unsafe_code)]

pub mod cleanup;
pub mod ring;

use std::path::Path;
use strata_classify::{attribute, Env, KindClassifier, Os, Role, RuleSet};
use strata_core::{scan, ScanControl, ScanOptions, ScanTree};
use strata_reclaim::{suggest, Group, Options, Target};
use strata_store::{AppRole, AppRow, DirOwner, ScanId, ScanMeta, Store, StoreError, SuggestionRow};

/// Which scanner produced a result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// Walking folders one by one.
    Walk,
    /// Reading the NTFS file table directly.
    FileTable,
}

/// A finished scan, and how it was done.
pub struct Outcome {
    pub tree: ScanTree,
    pub method: Method,
    /// Why the fast scan was not used when it was asked for.
    pub fallback_reason: Option<String>,
}

/// Scan `root`. With `prefer_fast` on Windows and a whole drive (`C:\`),
/// read the file table; if that is not possible (not NTFS, not running as
/// administrator, refused), walk the folders instead and say why.
pub fn scan_root(root: &Path, prefer_fast: bool, ctl: &ScanControl) -> std::io::Result<Outcome> {
    let mut fallback_reason = None;
    if prefer_fast {
        match fast_scan(root, ctl) {
            Some(Ok(tree)) => {
                return Ok(Outcome {
                    tree,
                    method: Method::FileTable,
                    fallback_reason: None,
                })
            }
            Some(Err(strata_mft::MftError::Cancelled)) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled",
                ))
            }
            Some(Err(e)) => fallback_reason = Some(e.to_string()),
            None => {}
        }
    }
    let tree = scan(&ScanOptions::new(root), &KindClassifier, ctl)?;
    Ok(Outcome {
        tree,
        method: Method::Walk,
        fallback_reason,
    })
}

/// The file-table scan, where it applies. `None` when `root` is not a
/// whole Windows drive (the walk is the only option then).
#[cfg(windows)]
pub fn fast_scan(root: &Path, ctl: &ScanControl) -> Option<Result<ScanTree, strata_mft::MftError>> {
    let text = root.to_string_lossy();
    let mut chars = text.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else {
        return None;
    };
    if !matches!(chars.as_str(), "" | "\\" | "/") || !letter.is_ascii_alphabetic() {
        return None;
    }
    Some(strata_mft::open_drive(letter).and_then(|drive| {
        let name = format!("{}:\\", letter.to_ascii_uppercase());
        strata_mft::scan_volume(
            drive,
            &strata_mft::MftOptions::new(name),
            &KindClassifier,
            ctl,
        )
    }))
}

/// The file-table scan, where it applies: never, except on Windows.
#[cfg(not(windows))]
pub fn fast_scan(
    _root: &Path,
    _ctl: &ScanControl,
) -> Option<Result<ScanTree, strata_mft::MftError>> {
    None
}

/// Scan a volume image (or device) through the file table. For testing and
/// the command line.
pub fn scan_image(image: &Path, ctl: &ScanControl) -> Result<ScanTree, strata_mft::MftError> {
    let file = std::fs::File::open(image)?;
    strata_mft::scan_volume(
        file,
        &strata_mft::MftOptions::new(image.as_os_str()),
        &KindClassifier,
        ctl,
    )
}

/// Save a scan with everything worked out from it: which app each folder
/// belongs to, and what could be cleaned up.
pub fn save_everything(
    store: &mut Store,
    tree: &ScanTree,
    meta: &ScanMeta,
) -> Result<ScanId, StoreError> {
    let id = store.save(tree, meta)?;
    let Some(env) = Env::current() else {
        return Ok(id);
    };
    let attribution = attribute(tree, &RuleSet::builtin(Os::current()), &env);
    let apps: Vec<AppRow> = attribution
        .apps
        .iter()
        .map(|a| AppRow {
            name: a.name.clone(),
            key: a.id.clone(),
            kind: a.kind,
            app_bytes: a.app_bytes,
            data_bytes: a.data_bytes,
            cache_bytes: a.cache_bytes,
        })
        .collect();
    let owners: Vec<Option<DirOwner>> = attribution
        .owners
        .iter()
        .map(|o| {
            o.map(|o| DirOwner {
                app: o.app,
                role: match o.role {
                    Role::App => AppRole::Program,
                    Role::Data => AppRole::Data,
                    Role::Cache => AppRole::Cache,
                },
            })
        })
        .collect();
    store.save_apps(id, &apps, &owners)?;

    if let Ok(policy) = strata_safety::Policy::for_current_user() {
        let rows: Vec<SuggestionRow> =
            suggest(tree, &attribution, &env, &policy, &Options::default())
                .into_iter()
                .map(|s| SuggestionRow {
                    group: group_key(s.group).into(),
                    contents_only: matches!(s.target, Target::Contents(_)),
                    path: s.target.path().to_path_buf(),
                    bytes: s.bytes,
                    app: s.app,
                    rebuildable: s.rebuildable,
                    reason: s.reason,
                })
                .collect();
        store.save_suggestions(id, &rows)?;
    }
    Ok(id)
}

/// The stable key a suggestion group is saved under.
pub fn group_key(group: Group) -> &'static str {
    match group {
        Group::AppCache => "app_cache",
        Group::DeveloperJunk => "developer_junk",
        Group::Cache => "cache",
        Group::OldInstaller => "old_installer",
        Group::OldDownload => "old_download",
    }
}

/// Seconds since 1970, for `ScanMeta`.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_and_saves_with_apps_and_suggestions() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        std::fs::create_dir_all(data.join("proj/node_modules/x")).unwrap();
        std::fs::write(data.join("proj/package.json"), b"{}").unwrap();
        std::fs::write(data.join("proj/node_modules/x/big.js"), vec![1u8; 300_000]).unwrap();

        let outcome = scan_root(&data, true, &ScanControl::new()).unwrap();
        // Not a whole Windows drive, so the walk is used without complaint.
        assert_eq!(outcome.method, Method::Walk);
        assert!(outcome.fallback_reason.is_none());

        let mut store = Store::open(&tmp.path().join("h.sqlite")).unwrap();
        let meta = ScanMeta {
            volume_id: "test".into(),
            started_at: 1,
            finished_at: 2,
        };
        let id = save_everything(&mut store, &outcome.tree, &meta).unwrap();
        assert_eq!(store.scan_info(id).unwrap().file_count, 2);
        assert!(store.children(id, 0).is_ok());
    }

    #[test]
    fn a_cancelled_walk_still_returns_what_it_has() {
        let tmp = tempfile::tempdir().unwrap();
        let ctl = ScanControl::new();
        ctl.cancel();
        let outcome = scan_root(tmp.path(), false, &ctl).unwrap();
        assert!(outcome.tree.stats.cancelled);
    }
}
