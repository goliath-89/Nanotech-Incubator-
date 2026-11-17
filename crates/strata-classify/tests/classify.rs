use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use strata_classify::{
    attribute, folder_kind, kind_of_name, known_extensions, Env, KindClassifier, Os, Role, RuleSet,
};
use strata_core::{
    scan, Classify, DirFlags, DirNode, Kind, ScanControl, ScanOptions, ScanTree, KIND_COUNT,
};
use strata_safety::{Platform, Policy};

// --- kinds -----------------------------------------------------------------

#[test]
fn extensions_map_to_kinds() {
    let cases: &[(&str, Kind)] = &[
        ("holiday.mp4", Kind::Videos),
        ("HOLIDAY.MOV", Kind::Videos),
        ("IMG_0001.HEIC", Kind::Photos),
        ("raw.CR3", Kind::Photos),
        ("song.flac", Kind::Music),
        ("report.final.v2.pdf", Kind::Documents),
        ("backup.tar.gz", Kind::Documents),
        ("setup.exe", Kind::Apps),
        ("Installer.dmg", Kind::Apps),
        ("level1.pak", Kind::Games),
        ("main.rs", Kind::Developer),
        ("data.sqlite3", Kind::Developer),
        ("pagefile.sys", Kind::System),
        ("Docker.raw", Kind::Developer),
        ("download.crdownload", Kind::Caches),
        ("Thumbs.db", Kind::Caches),
        ("README", Kind::Other),
        ("noext.", Kind::Other),
        (".bashrc", Kind::Other),
        ("weird.thisextensionistoolongtobereal", Kind::Other),
        ("unknown.zzz", Kind::Other),
        ("", Kind::Other),
    ];
    for (name, kind) in cases {
        assert_eq!(kind_of_name(name.as_bytes()), *kind, "{name}");
    }
    assert!(
        known_extensions() >= 550,
        "only {} extensions",
        known_extensions()
    );
    // Names that are not valid text must not panic.
    assert_eq!(
        kind_of_name(&[0xFF, 0xFE, b'.', b'm', b'p', b'4']),
        Kind::Videos
    );
}

#[test]
fn a_folders_location_decides_before_file_names_do() {
    let cases: &[(&str, Option<Kind>)] = &[
        ("/Users/sam/Movies", None),
        ("/Users/sam/Documents/Taxes 2025", None),
        ("/Users/sam/Library/Application Support/Slack", None),
        (
            "/Users/sam/Library/Caches/com.apple.Safari",
            Some(Kind::Caches),
        ),
        (
            "/Users/sam/Library/Application Support/Slack/Cache/data",
            Some(Kind::Caches),
        ),
        (
            "/Users/sam/code/app/node_modules/react/dist",
            Some(Kind::Developer),
        ),
        (
            "/Users/sam/code/app/node_modules/.cache/babel",
            Some(Kind::Developer),
        ),
        (
            "/Users/sam/Library/Developer/Xcode/DerivedData/App",
            Some(Kind::Developer),
        ),
        (
            "/Users/sam/Library/Containers/com.docker.docker/Data",
            Some(Kind::Developer),
        ),
        (
            "/Applications/Xcode.app/Contents/Developer/Platforms",
            Some(Kind::Apps),
        ),
        (
            "/Users/sam/Applications/Tool.app/Contents",
            Some(Kind::Apps),
        ),
        (
            "/Users/sam/Library/Application Support/Steam/steamapps/common/Game",
            Some(Kind::Games),
        ),
        (
            "/Program Files (x86)/Steam/steamapps/common/Game/data",
            Some(Kind::Games),
        ),
        ("/Program Files/Git/bin", Some(Kind::Apps)),
        ("/System/Library/Frameworks", Some(Kind::System)),
        (
            "/Library/Caches/com.apple.iconservices.store",
            Some(Kind::Caches),
        ),
        ("/opt/homebrew/Cellar/ffmpeg", Some(Kind::Developer)),
        ("/Windows/System32", Some(Kind::System)),
        ("/Users/sam/Windows/notes", None),
        ("/Users/sam/Library", None),
        // Ordinary words people use for their own folders are not markers there.
        ("/Users/sam/Documents/Applications/2026/cover-letter", None),
        ("/Users/sam/Documents/Temp", None),
        ("/Users/sam/Pictures/cache", None),
        ("/Users/sam/Music/Pods", None),
        ("/Users/sam/Desktop/Program Files", None),
        ("/Users/sam/OneDrive - Work/tmp", None),
        ("/home/sam/Videos/Cache", None),
        // Strong markers still count inside personal folders.
        (
            "/Users/sam/Documents/code/app/node_modules",
            Some(Kind::Developer),
        ),
        ("/Users/sam/Documents/Tool.app/Contents", Some(Kind::Apps)),
        ("/Users/sam/Downloads/steamapps/common", Some(Kind::Games)),
        // The real programs folders, and real caches outside personal folders.
        ("/Users/sam/Applications", Some(Kind::Apps)),
        ("/Applications/Utilities", Some(Kind::Apps)),
        ("/Users/sam/AppData/Local/Temp/x", Some(Kind::Caches)),
        ("/Users/sam/code/app/tmp", Some(Kind::Caches)),
        ("/Volumes/Ext/Backups/Program Files/x", None),
    ];
    for (path, kind) in cases {
        assert_eq!(folder_kind(Path::new(path)), *kind, "{path}");
    }
}

#[test]
fn a_real_scan_gets_kind_totals() {
    // Not the system temp folder: everything under a folder named `tmp` is cache.
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).parent().unwrap();
    let tmp = tempfile::tempdir_in(target).unwrap();
    let put = |rel: &str, bytes: usize| {
        let p = tmp.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![1u8; bytes]).unwrap();
    };
    put("Movies/trip.mp4", 400_000);
    put("Pictures/a.jpg", 100_000);
    put("proj/node_modules/lib/index.js", 50_000);
    put("proj/node_modules/lib/logo.png", 50_000);
    put("stuff/Cache/video.mp4", 200_000);
    put("stuff/mystery.xyz", 10_000);
    let tree = scan(
        &ScanOptions::new(tmp.path()),
        &KindClassifier,
        &ScanControl::new(),
    )
    .unwrap();
    let k = tree.root().kind_bytes;
    let at_least = |kind: Kind, bytes: u64| {
        assert!(k[kind as usize] >= bytes, "{kind:?}: {}", k[kind as usize])
    };
    at_least(Kind::Videos, 400_000);
    assert!(
        k[Kind::Videos as usize] < 600_000,
        "the cached video is cache, not video"
    );
    at_least(Kind::Photos, 100_000);
    at_least(Kind::Developer, 100_000);
    at_least(Kind::Caches, 200_000);
    at_least(Kind::Other, 10_000);
    assert!(KindClassifier
        .folder_kind(&tmp.path().join("Movies"))
        .is_none());
}

// --- rules -----------------------------------------------------------------

#[test]
fn built_in_rules_load() {
    let mac = RuleSet::builtin(Os::MacOs);
    let win = RuleSet::builtin(Os::Windows);
    assert!(mac.apps.len() >= 20 && win.apps.len() >= 19);
    assert!(mac
        .apps
        .iter()
        .any(|a| a.name == "Docker" && a.kind == Some(Kind::Developer)));
    assert!(RuleSet::builtin(Os::Linux).apps.is_empty());
}

fn instantiate(pattern: &str, home: &str, drive: &str) -> String {
    let filled: Vec<String> = pattern
        .split('/')
        .filter(|p| !p.is_empty())
        .map(|p| {
            if p == "~" {
                home.to_string()
            } else if p.ends_with(':') {
                drive.to_string()
            } else if p == "*" {
                "x".to_string()
            } else if let Some(prefix) = p.strip_suffix('*') {
                format!("{prefix}x")
            } else if let Some(suffix) = p.strip_prefix('*') {
                format!("x{suffix}")
            } else {
                p.to_string()
            }
        })
        .collect();
    let joined = filled.join("/");
    if pattern.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

/// The contract with strata-safety: a `cache` rule can only name a folder
/// whose contents the safety policy already allows to be moved to the Trash.
/// A rules file can never widen what may be removed.
#[test]
fn every_cache_rule_is_inside_a_place_the_safety_policy_allows() {
    for (os, platform, home, drive) in [
        (Os::MacOs, Platform::MacOs, "/Users/sam", ""),
        (Os::Windows, Platform::Windows, "C:/Users/sam", "C:"),
    ] {
        let policy = Policy::new(platform, &[]).unwrap();
        for app in RuleSet::builtin(os).apps {
            for (pattern, role) in &app.paths {
                let folder = instantiate(pattern, home, drive);
                if *role == Role::Cache {
                    let item = format!("{folder}/item");
                    if let Err(why) = policy.check_str(&item) {
                        panic!(
                            "{}: cache rule {pattern:?} is not removable: {why}",
                            app.name
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn bad_rule_files_are_rejected_whole() {
    let bad = [
        ("[[app]]\nname = \"A\"\nid = \"a\"\n", "lists no folders"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"Library/X\"]\n", "must start with"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/Library/../..\"]\n", "must not contain"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/Lib\\\\X\"]\n", "must use /"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"/Users\"]\n", "too broad"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/*\"]\n", "too broad"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/Library/*\"]\n", "bare *"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/Li*br*ary/X\"]\n", "one *"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\nkind = \"Stuff\"\ndata = [\"~/Library/X\"]\n", "unknown kind"),
        ("[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/Library/X\"]\nopen = true\n", "unknown field"),
        (
            "[[app]]\nname = \"A\"\nid = \"a\"\ndata = [\"~/Library/X\"]\n[[app]]\nname = \"B\"\nid = \"a\"\ndata = [\"~/Library/Y\"]\n",
            "used twice",
        ),
    ];
    for (text, expected) in bad {
        let err = RuleSet::from_toml(text).expect_err(text);
        assert!(err.contains(expected), "{text:?} gave {err:?}");
    }
}

#[test]
fn newer_rules_replace_older_ones_by_id() {
    let mut rules =
        RuleSet::from_toml("[[app]]\nname = \"Old\"\nid = \"x\"\ndata = [\"~/Library/Old\"]\n")
            .unwrap();
    rules.merge(
        RuleSet::from_toml(
            "[[app]]\nname = \"New\"\nid = \"x\"\ndata = [\"~/Library/New\"]\n[[app]]\nname = \"Extra\"\nid = \"y\"\ndata = [\"~/Library/Extra\"]\n",
        )
        .unwrap(),
    );
    let names: Vec<_> = rules.apps.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["New", "Extra"]);
}

// --- attribution -----------------------------------------------------------

#[derive(Default)]
struct Trie {
    own: u64,
    children: BTreeMap<String, Trie>,
}

/// Build a scan result from "folder path -> bytes of files directly in it".
fn tree(root: &str, folders: &[(&str, u64)]) -> ScanTree {
    let mut top = Trie::default();
    for (path, bytes) in folders {
        let mut at = &mut top;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            at = at.children.entry(part.to_string()).or_default();
        }
        at.own += bytes;
    }
    fn emit(node: &Trie, name: OsString, parent: Option<u32>, dirs: &mut Vec<DirNode>) -> u32 {
        let id = dirs.len() as u32;
        dirs.push(DirNode {
            parent,
            name,
            children: Vec::new(),
            alloc_bytes: node.own,
            logical_bytes: node.own,
            file_count: 0,
            dir_count: 0,
            kind_bytes: [0; KIND_COUNT],
            cloud_only_bytes: 0,
            own_files_alloc: node.own,
            own_file_count: 0,
            flags: DirFlags::default(),
        });
        for (child_name, child) in &node.children {
            let cid = emit(child, OsString::from(child_name), Some(id), dirs);
            dirs[id as usize].children.push(cid);
            dirs[id as usize].alloc_bytes += dirs[cid as usize].alloc_bytes;
        }
        id
    }
    let mut dirs = Vec::new();
    emit(&top, OsString::from(root), None, &mut dirs);
    ScanTree {
        dirs,
        files: Vec::new(),
        errors: Vec::new(),
        stats: Default::default(),
    }
}

fn mac_env() -> Env {
    Env {
        os: Os::MacOs,
        home: "/Users/sam".into(),
        system_drive: String::new(),
    }
}

fn dir_id(t: &ScanTree, path: &str) -> usize {
    (0..t.dirs.len())
        .find(|&i| t.dir_path(i as u32) == Path::new(path))
        .unwrap_or_else(|| panic!("{path}"))
}

const GB: u64 = 1_000_000_000;

#[test]
fn folders_are_attributed_to_apps_by_role() {
    let t = tree(
        "/",
        &[
            ("Applications/Docker.app/Contents", 2 * GB),
            ("Applications/Mystery Tool.app/Contents", GB),
            ("Applications/Loose Folder", GB),
            (
                "Users/sam/Library/Containers/com.docker.docker/Data/vms",
                60 * GB,
            ),
            ("Users/sam/Library/Caches/com.docker.docker", 3 * GB),
            ("Users/sam/Library/Application Support/Slack", 4 * GB),
            (
                "Users/sam/Library/Application Support/Slack/Cache/deep/er",
                5 * GB,
            ),
            ("Users/sam/Library/Application Support/Slack/Cache", GB),
            ("Users/sam/Documents/Work", 9 * GB),
            (
                "Users/sam/Pictures/Photos Library.photoslibrary/originals",
                100 * GB,
            ),
            ("Users/sam/Pictures/Old.PHOTOSLIBRARY/originals", 10 * GB),
            ("users/SAM/library/caches/COM.SPOTIFY.CLIENT", 7 * GB),
        ],
    );
    let a = attribute(&t, &RuleSet::builtin(Os::MacOs), &mac_env());

    let app = |name: &str| {
        a.apps
            .iter()
            .find(|x| x.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    let docker = app("Docker");
    assert_eq!(
        (docker.app_bytes, docker.data_bytes, docker.cache_bytes),
        (2 * GB, 60 * GB, 3 * GB)
    );
    let slack = app("Slack");
    assert_eq!(
        (slack.app_bytes, slack.data_bytes, slack.cache_bytes),
        (0, 4 * GB, 6 * GB)
    );
    assert_eq!(
        app("Photos").data_bytes,
        110 * GB,
        "the * rule matches both libraries, whatever their case"
    );
    assert_eq!(
        app("Spotify").cache_bytes,
        7 * GB,
        "names compare without regard to case on macOS"
    );
    assert_eq!(
        app("Mystery Tool").app_bytes,
        GB,
        "an app no rule names is still an app"
    );
    assert!(!a.apps.iter().any(|x| x.name.contains("Loose")));
    assert!(
        !a.apps.iter().any(|x| x.name == "Xcode"),
        "apps that own nothing are left out"
    );

    let names: Vec<_> = a.apps.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(&names[..3], ["Photos", "Docker", "Slack"], "largest first");

    // Owners: inherited downwards, overridden by the deeper cache rule, absent elsewhere.
    let owner = |path: &str| {
        a.owners[dir_id(&t, path)].map(|o| (a.apps[o.app as usize].name.as_str(), o.role))
    };
    assert_eq!(
        owner("/Users/sam/Library/Containers/com.docker.docker/Data/vms"),
        Some(("Docker", Role::Data))
    );
    assert_eq!(
        owner("/Users/sam/Library/Application Support/Slack"),
        Some(("Slack", Role::Data))
    );
    assert_eq!(
        owner("/Users/sam/Library/Application Support/Slack/Cache/deep/er"),
        Some(("Slack", Role::Cache))
    );
    assert_eq!(owner("/Users/sam/Documents/Work"), None);
    assert_eq!(owner("/Users/sam/Library"), None);
    assert_eq!(owner("/Applications/Loose Folder"), None);

    let cache_paths: Vec<_> = a.cache_dirs.iter().map(|&d| t.dir_path(d)).collect();
    assert!(cache_paths.contains(&"/Users/sam/Library/Caches/com.docker.docker".into()));
    assert!(cache_paths.contains(&"/Users/sam/Library/Application Support/Slack/Cache".into()));
    assert_eq!(cache_paths.len(), 3);

    // Every attributed byte is counted exactly once.
    let attributed: u64 = a.apps.iter().map(|x| x.total_bytes()).sum();
    assert_eq!(attributed, t.root().alloc_bytes - 9 * GB - GB);
}

#[test]
fn works_when_only_part_of_the_drive_was_scanned() {
    // Scanned the home folder.
    let t = tree(
        "/Users/sam",
        &[
            ("Library/Caches/com.docker.docker", 3 * GB),
            ("Documents", GB),
        ],
    );
    let a = attribute(&t, &RuleSet::builtin(Os::MacOs), &mac_env());
    assert_eq!(a.apps.len(), 1);
    assert_eq!(a.apps[0].cache_bytes, 3 * GB);

    // Scanned a folder inside an app's data: all of it is that app's.
    let t = tree(
        "/Users/sam/Library/Containers/com.docker.docker/Data",
        &[("vms/0", 5 * GB), ("log", GB)],
    );
    let a = attribute(&t, &RuleSet::builtin(Os::MacOs), &mac_env());
    assert_eq!(a.apps.len(), 1);
    assert_eq!(a.apps[0].data_bytes, 6 * GB);
    assert!(a.owners.iter().all(|o| o.is_some()));

    // Scanned somewhere unrelated: nothing is attributed.
    let t = tree(
        "/Volumes/Backup",
        &[("Library/Caches/com.docker.docker", 3 * GB)],
    );
    assert!(attribute(&t, &RuleSet::builtin(Os::MacOs), &mac_env())
        .apps
        .is_empty());
}

#[test]
fn windows_rules_use_the_system_drive_and_profile() {
    let env = Env {
        os: Os::Windows,
        home: r"D:\Users\sam".into(),
        system_drive: "D:".into(),
    };
    let t = tree(
        r"D:\",
        &[
            ("Program Files/Google/Chrome/Application", GB),
            ("Program Files/Some Vendor/Tool", 2 * GB),
            ("Program Files (x86)/Steam/steamapps/common/Game", 80 * GB),
            (
                "Users/sam/AppData/Local/Google/Chrome/User Data/Default",
                3 * GB,
            ),
            (
                "Users/sam/AppData/Local/Google/Chrome/User Data/Default/Cache/Cache_Data",
                4 * GB,
            ),
            (
                "Users/sam/AppData/Local/Google/Chrome/User Data/Profile 1/Code Cache/js",
                GB,
            ),
            ("Users/sam/AppData/Local/Temp/junk", 5 * GB),
            ("Users/sam/AppData/Local/Programs/Obsidian", GB),
            ("Windows/Temp", 2 * GB),
            ("Users/sam/Documents", 9 * GB),
            ("Program Files/Common Files/System", GB),
            ("Program Files (x86)/Common Files/Oracle", GB),
            ("Program Files/Vendor X/bin", GB),
            ("Program Files (x86)/vendor x/bin", GB),
        ],
    );
    let a = attribute(&t, &RuleSet::builtin(Os::Windows), &env);
    let app = |name: &str| {
        a.apps
            .iter()
            .find(|x| x.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    let chrome = app("Google Chrome");
    assert_eq!(
        (chrome.app_bytes, chrome.data_bytes, chrome.cache_bytes),
        (GB, 3 * GB, 5 * GB)
    );
    assert_eq!(app("Steam").app_bytes, 80 * GB);
    assert_eq!(app("Steam").kind, Some(Kind::Games));
    assert_eq!(app("Temporary files").cache_bytes, 7 * GB);
    assert_eq!(
        app("Some Vendor").app_bytes,
        2 * GB,
        "an unlisted Program Files folder is an app by its name"
    );
    assert_eq!(app("Obsidian").app_bytes, GB);
    assert!(
        !a.apps.iter().any(|x| x.name == "Google"),
        "a listed app is not also reported by folder name"
    );
}

#[test]
fn attribution_on_a_real_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let put = |rel: &str, bytes: usize| {
        let p = home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![1u8; bytes]).unwrap();
    };
    put(".toolbox/data/big.bin", 300_000);
    put(".toolbox/cache/blob", 200_000);
    put("Documents/notes.txt", 1_000);
    let rules = RuleSet::from_toml(
        "[[app]]\nname = \"Toolbox\"\nid = \"toolbox\"\nkind = \"Developer\"\ndata = [\"~/.toolbox\"]\ncache = [\"~/.toolbox/cache\"]\n",
    )
    .unwrap();
    let env = Env {
        os: Os::Linux,
        home: home.to_string_lossy().into_owned(),
        system_drive: String::new(),
    };
    // Scan from above the home folder, as a whole-drive scan would.
    let scanned = scan(
        &ScanOptions::new(tmp.path()),
        &KindClassifier,
        &ScanControl::new(),
    )
    .unwrap();
    let a = attribute(&scanned, &rules, &env);
    assert_eq!(a.apps.len(), 1);
    assert!(a.apps[0].data_bytes >= 300_000 && a.apps[0].cache_bytes >= 200_000);
    assert!(a.apps[0].data_bytes < 400_000);
    assert_eq!(a.cache_dirs.len(), 1);
    assert_eq!(
        scanned.dirs[a.cache_dirs[0] as usize].name,
        OsStr::new("cache")
    );
}
