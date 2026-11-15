//! Volume listing, checked on whatever system the tests run on. The macOS
//! and Windows code is only ever exercised by those systems' CI runs.

use std::collections::HashSet;
use strata_core::{volume_usage, volumes, VolumeKind};

#[test]
fn lists_the_volume_this_test_runs_on() {
    let all = volumes().unwrap();
    assert!(!all.is_empty());
    for v in &all {
        println!("{v:?}");
        assert!(
            !v.id.is_empty() && !v.name.is_empty() && !v.file_system.is_empty(),
            "{v:?}"
        );
        assert!(v.mount.is_absolute(), "{v:?}");
        assert!(
            v.usage.total_bytes > 0 && v.usage.free_bytes <= v.usage.total_bytes,
            "{v:?}"
        );
        assert!(v.usage.available_bytes <= v.usage.total_bytes, "{v:?}");
    }
    let ids: HashSet<&str> = all.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids.len(), all.len(), "ids are unique");
    assert!(all.iter().any(|v| v.kind == VolumeKind::Internal));
    // Internal volumes come first.
    let first_other = all
        .iter()
        .position(|v| v.kind != VolumeKind::Internal)
        .unwrap_or(all.len());
    assert!(all[first_other..]
        .iter()
        .all(|v| v.kind != VolumeKind::Internal));

    // The folder this test runs in is on one of them, and the listed
    // figures agree with asking about that folder directly.
    let here = std::env::current_dir().unwrap().canonicalize().unwrap();
    let home = all
        .iter()
        .filter(|v| {
            let mount = v.mount.canonicalize().unwrap_or_else(|_| v.mount.clone());
            here.starts_with(&mount)
        })
        .max_by_key(|v| v.mount.as_os_str().len());
    // A build folder on a volume the list hides (an overlay, a ramdisk)
    // is fine; when it is listed, the numbers must match.
    if let Some(v) = home {
        let direct = volume_usage(&here).unwrap();
        assert_eq!(direct.total_bytes, v.usage.total_bytes, "{v:?}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_root_volume_has_a_real_name_and_uuid() {
    let all = volumes().unwrap();
    let root = all
        .iter()
        .find(|v| v.mount == std::path::Path::new("/"))
        .expect("the root volume is listed");
    assert_eq!(root.kind, VolumeKind::Internal);
    assert_eq!(root.file_system, "apfs");
    assert!(root.name != "/" && !root.name.contains('/'), "{root:?}");
    let parts: Vec<&str> = root.id.split('-').collect();
    assert_eq!(
        parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
        [8, 4, 4, 4, 12],
        "{root:?}"
    );
    assert!(
        root.id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()),
        "{root:?}"
    );
    // Helper volumes that Finder hides are not listed.
    assert!(
        !all.iter().any(|v| v.mount.starts_with("/System/Volumes")),
        "{all:?}"
    );
}

#[cfg(windows)]
#[test]
fn windows_system_drive_is_listed_with_a_serial_number() {
    let all = volumes().unwrap();
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let system = all
        .iter()
        .find(|v| {
            v.mount
                .to_string_lossy()
                .eq_ignore_ascii_case(&format!("{drive}\\"))
        })
        .expect("the system drive is listed");
    assert_eq!(system.kind, VolumeKind::Internal);
    assert_eq!(system.file_system, "ntfs");
    assert_eq!(system.id.len(), 8, "{system:?}");
    assert!(
        system.id.chars().all(|c| c.is_ascii_hexdigit()),
        "{system:?}"
    );
}
