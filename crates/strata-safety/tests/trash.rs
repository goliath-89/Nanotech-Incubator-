//! Real moves to the real Trash, inside a scratch folder in the home folder.

use std::fs;
use std::path::{Path, PathBuf};
use strata_safety::{trash, vet, Policy, Reason, VetError};

/// A scratch folder that the policy allows work in on every platform: a
/// hidden folder at the top of the home folder (the folder itself is
/// protected, its contents are not).
struct Scratch {
    _dir: tempfile::TempDir,
    root: PathBuf,
    policy: Policy,
}

fn scratch() -> Scratch {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .expect("a home folder");
    let dir = tempfile::Builder::new()
        .prefix(".strata-test-")
        .tempdir_in(home)
        .unwrap();
    let root = dir.path().join("scanned");
    fs::create_dir(&root).unwrap();
    Scratch {
        root,
        policy: Policy::for_current_user().unwrap(),
        _dir: dir,
    }
}

fn file(path: &Path) -> PathBuf {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"contents").unwrap();
    path.to_path_buf()
}

fn refusal(result: Result<PathBuf, VetError>) -> Reason {
    match result {
        Err(VetError::Refused(r)) => r.reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn moves_files_and_folders_to_the_trash() {
    let s = scratch();
    let a = file(&s.root.join("old-installer.dmg"));
    let b = file(&s.root.join("cache/blob.bin"))
        .parent()
        .unwrap()
        .to_path_buf();
    let keep = file(&s.root.join("keep.txt"));

    let report = trash(&s.policy, &s.root, &[a.clone(), b.clone()]);
    assert!(
        report.refused.is_empty() && report.failed.is_empty(),
        "{report:?}"
    );
    assert_eq!(report.moved, [a.clone(), b.clone()]);
    assert!(!a.exists() && !b.exists());
    assert!(keep.exists());
}

#[test]
fn a_folder_and_something_inside_it_are_one_move() {
    let s = scratch();
    let inner = file(&s.root.join("build/output/app.bin"));
    let folder = s.root.join("build");
    let report = trash(&s.policy, &s.root, &[inner.clone(), folder.clone()]);
    assert!(
        report.failed.is_empty() && report.refused.is_empty(),
        "{report:?}"
    );
    assert_eq!(report.moved.len(), 2);
    assert!(!folder.exists());
}

#[test]
fn refuses_the_scan_root_and_anything_outside_it() {
    let s = scratch();
    let outside = file(&s.root.parent().unwrap().join("not-scanned/file.txt"));
    let inside = file(&s.root.join("ok.txt"));

    assert_eq!(
        refusal(vet(&s.policy, &s.root, &s.root)),
        Reason::OutsideScan
    );
    assert_eq!(
        refusal(vet(&s.policy, &s.root, &outside)),
        Reason::OutsideScan
    );

    let report = trash(
        &s.policy,
        &s.root,
        &[outside.clone(), s.root.clone(), inside.clone()],
    );
    assert_eq!(report.refused.len(), 2);
    assert_eq!(
        report.moved,
        std::slice::from_ref(&inside),
        "a refusal does not stop the allowed items"
    );
    assert!(outside.exists() && s.root.exists() && !inside.exists());
}

#[test]
fn refuses_protected_places_even_when_asked_to_scan_them() {
    let s = scratch();
    let home = PathBuf::from(
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap(),
    );
    // Pretend the whole home folder was scanned and its top-level folders picked.
    for name in ["Documents", "Desktop", ".ssh"] {
        let target = home.join(name);
        match vet(&s.policy, &home, &target) {
            Err(VetError::Refused(_)) => {}
            other => panic!("{} must be refused, got {other:?}", target.display()),
        }
    }
    // Only `vet` here: a test must never be one bug away from binning a home folder.
    match vet(&s.policy, home.parent().unwrap(), &home) {
        Err(VetError::Refused(_)) => {}
        other => panic!("the home folder must be refused, got {other:?}"),
    }
}

#[test]
fn something_already_gone_is_reported_not_fatal() {
    let s = scratch();
    let real = file(&s.root.join("real.txt"));
    let ghost = s.root.join("ghost.txt");
    let report = trash(&s.policy, &s.root, &[ghost.clone(), real.clone()]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, ghost);
    assert_eq!(report.moved, [real]);
}

#[cfg(unix)]
mod links {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn a_link_is_never_moved() {
        let s = scratch();
        let target = file(&s.root.join("target.txt"));
        let link = s.root.join("link.txt");
        symlink(&target, &link).unwrap();
        assert_eq!(refusal(vet(&s.policy, &s.root, &link)), Reason::Link);
        let report = trash(&s.policy, &s.root, std::slice::from_ref(&link));
        assert_eq!(report.refused.len(), 1);
        assert!(target.exists() && fs::symlink_metadata(&link).is_ok());
    }

    #[test]
    fn a_linked_folder_cannot_smuggle_a_path_out_of_the_scan() {
        let s = scratch();
        let elsewhere = s.root.parent().unwrap().join("elsewhere");
        let victim = file(&elsewhere.join("precious.txt"));
        symlink(&elsewhere, s.root.join("innocent")).unwrap();

        // As text, this path is inside the scanned folder.
        let disguised = s.root.join("innocent/precious.txt");
        assert_eq!(
            refusal(vet(&s.policy, &s.root, &disguised)),
            Reason::OutsideScan
        );
        let report = trash(&s.policy, &s.root, &[disguised]);
        assert!(report.moved.is_empty());
        assert!(victim.exists());
    }

    #[test]
    fn a_linked_folder_cannot_smuggle_a_path_into_a_protected_place() {
        let mut s = scratch();
        // A protected folder that happens to sit inside the scanned folder.
        let vault = s.root.join("vault");
        let victim = file(&vault.join("keys.db"));
        s.policy.protect(&vault.to_string_lossy()).unwrap();
        symlink(&vault, s.root.join("cache")).unwrap();

        let disguised = s.root.join("cache/keys.db");
        assert_eq!(
            refusal(vet(&s.policy, &s.root, &disguised)),
            Reason::Protected
        );
        let report = trash(&s.policy, &s.root, &[disguised, vault.clone()]);
        assert_eq!(report.refused.len(), 2);
        assert!(victim.exists());
    }

    #[test]
    fn a_scan_root_given_through_a_link_still_works() {
        let s = scratch();
        let alias = s.root.parent().unwrap().join("alias");
        symlink(&s.root, &alias).unwrap();
        let item = file(&s.root.join("junk.tmp"));
        let report = trash(&s.policy, &alias, &[alias.join("junk.tmp")]);
        assert!(
            report.refused.is_empty() && report.failed.is_empty(),
            "{report:?}"
        );
        assert!(!item.exists());
    }
}
