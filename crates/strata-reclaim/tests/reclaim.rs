use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use strata_classify::{attribute, Env, KindClassifier, Os, RuleSet};
use strata_core::{scan, DirFlags, DirNode, ScanControl, ScanOptions, ScanTree, KIND_COUNT};
use strata_reclaim::{expand, find_duplicates, suggest, DuplicateControl, Group, Options, Target};
use strata_safety::{Platform, Policy};

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

// --- suggestions on a hand-built macOS scan ---------------------------------

#[derive(Default)]
struct Trie {
    own: u64,
    children: BTreeMap<String, Trie>,
}

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
    ScanTree { dirs, files: Vec::new(), errors: Vec::new(), stats: Default::default() }
}

#[test]
fn suggests_caches_and_developer_folders_once_each_and_only_removable_ones() {
    let t = tree(
        "/",
        &[
            ("Users/sam/Library/Caches/com.docker.docker", 3 * GB),
            ("Users/sam/Library/Application Support/Slack", 4 * GB),
            ("Users/sam/Library/Application Support/Slack/Cache", GB),
            ("Users/sam/Library/Caches/Google/Chrome/Default", 4 * GB),
            ("Users/sam/Library/Caches/com.unknown.tool", 2 * GB),
            ("Users/sam/Library/Caches/com.looks-like-an.app", 2 * GB),
            ("Users/sam/Library/Caches/tiny", MB),
            ("Users/sam/Library/Logs/DiagnosticReports", 50 * MB),
            ("Users/sam/code/app/node_modules/react", 400 * MB),
            ("Users/sam/code/app/node_modules/react/node_modules/dep", 100 * MB),
            ("Users/sam/code/rs/target/debug", 2 * GB),
            ("Users/sam/.npm/_cacache", GB),
            ("Users/sam/.gradle/caches/modules-2", 2 * GB),
            ("Applications/Tool.app/Contents/Resources/node_modules", GB),
            ("Users/sam/Documents/Work", 9 * GB),
            ("Users/sam/Library/Application Support/Steam/steamapps", 70 * GB),
        ],
    );
    let env = Env { os: Os::MacOs, home: "/Users/sam".into(), system_drive: String::new() };
    let policy = Policy::new(Platform::MacOs, &[]).unwrap();
    let attribution = attribute(&t, &RuleSet::builtin(Os::MacOs), &env);
    let found = suggest(&t, &attribution, &env, &policy, &Options::default());

    let got: Vec<(Group, String, u64)> = found
        .iter()
        .map(|s| {
            let shown = match &s.target {
                Target::Contents(p) => format!("{}/*", p.display()),
                Target::Item(p) => p.display().to_string(),
            };
            (s.group, shown, s.bytes)
        })
        .collect();
    let expected: Vec<(Group, String, u64)> = [
        (Group::AppCache, "/Users/sam/Library/Caches/Google/Chrome/*", 4 * GB),
        (Group::AppCache, "/Users/sam/Library/Caches/com.docker.docker/*", 3 * GB),
        (Group::AppCache, "/Users/sam/.npm/_cacache/*", GB),
        (Group::AppCache, "/Users/sam/Library/Application Support/Slack/Cache/*", GB),
        (Group::DeveloperJunk, "/Users/sam/.gradle/caches/*", 2 * GB),
        (Group::DeveloperJunk, "/Users/sam/code/app/node_modules", 500 * MB),
        (Group::Cache, "/Users/sam/Library/Caches/com.unknown.tool", 2 * GB),
        (Group::Cache, "/Users/sam/Library/Logs/DiagnosticReports", 50 * MB),
    ]
    .into_iter()
    .map(|(g, p, b)| (g, p.to_string(), b))
    .collect();
    assert_eq!(got, expected);

    assert!(found.iter().all(|s| s.rebuildable));
    assert_eq!(found[1].app.as_deref(), Some("Docker"));
    assert!(found[1].reason.contains("Docker"));
    // Nothing overlaps, so the sum is what would really be freed.
    let total: u64 = found.iter().map(|s| s.bytes).sum();
    assert_eq!(total, 13 * GB + 500 * MB + 50 * MB);
    // The rust `target` folder is absent: no Cargo.toml beside it proves what it is.
    // The node_modules inside an app is absent: the policy protects apps. So is the
    // cache folder whose name ends in `.app`: the policy cannot tell it from a program,
    // and a missed suggestion is the safe mistake.
}

// --- suggestions and cleanup on real files ------------------------------------

struct Home {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    env: Env,
    policy: Policy,
}

fn home() -> Home {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/sam");
    fs::create_dir_all(&home).unwrap();
    let text = home.to_string_lossy().into_owned();
    Home {
        env: Env { os: Os::Linux, home: text.clone(), system_drive: String::new() },
        policy: Policy::new(Platform::Linux, &[&text]).unwrap(),
        home,
        _tmp: tmp,
    }
}

fn put(path: &Path, bytes: usize, age_days: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![3u8; bytes]).unwrap();
    let when = SystemTime::now() - Duration::from_secs(age_days * 86_400 + 60);
    File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
}

fn scan_small(root: &Path) -> ScanTree {
    let mut opts = ScanOptions::new(root);
    opts.min_file_bytes = 64 * 1024;
    scan(&opts, &KindClassifier, &ScanControl::new()).unwrap()
}

fn small_options() -> Options {
    Options { min_bytes: 100 * 1024, ..Options::default() }
}

#[test]
fn stale_downloads_and_marked_build_folders_are_found_on_disk() {
    let h = home();
    put(&h.home.join("Downloads/old-setup.dmg"), 300_000, 30);
    put(&h.home.join("Downloads/new-setup.dmg"), 300_000, 2);
    put(&h.home.join("Downloads/old-video.mp4"), 400_000, 200);
    put(&h.home.join("Downloads/recent-video.mp4"), 400_000, 20);
    put(&h.home.join("Documents/old-but-not-a-download.mp4"), 400_000, 500);
    put(&h.home.join("proj/Cargo.toml"), 100, 0);
    put(&h.home.join("proj/target/debug/app"), 500_000, 0);
    put(&h.home.join("notes/target/keep-me.bin"), 500_000, 0);

    let t = scan_small(&h.home);
    let rules = RuleSet::default();
    let found = suggest(&t, &attribute(&t, &rules, &h.env), &h.env, &h.policy, &small_options());
    let got: Vec<(Group, PathBuf, bool)> =
        found.iter().map(|s| (s.group, s.target.path().strip_prefix(&h.home).unwrap().to_path_buf(), s.rebuildable)).collect();
    assert_eq!(
        got,
        [
            (Group::DeveloperJunk, "proj/target".into(), true),
            (Group::OldInstaller, "Downloads/old-setup.dmg".into(), false),
            (Group::OldDownload, "Downloads/old-video.mp4".into(), false),
        ]
    );
    assert!(found[1].reason.contains("30 days"));
}

#[test]
fn a_cache_suggestion_expands_to_its_contents_and_cleans_up_through_the_safety_engine() {
    let h = home();
    put(&h.home.join(".toolbox/cache/a/blob1"), 200_000, 0);
    put(&h.home.join(".toolbox/cache/b.bin"), 200_000, 0);
    put(&h.home.join(".toolbox/settings.json"), 100, 0);
    let rules = RuleSet::from_toml(
        "[[app]]\nname = \"Toolbox\"\nid = \"toolbox\"\ndata = [\"~/.toolbox\"]\ncache = [\"~/.toolbox/cache\"]\n",
    )
    .unwrap();
    let t = scan_small(&h.home);
    let found = suggest(&t, &attribute(&t, &rules, &h.env), &h.env, &h.policy, &small_options());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].group, Group::AppCache);
    assert_eq!(found[0].target, Target::Contents(h.home.join(".toolbox/cache")));

    let items = expand(&found[0].target).unwrap();
    assert_eq!(items, [h.home.join(".toolbox/cache/a"), h.home.join(".toolbox/cache/b.bin")]);
    let report = strata_safety::trash(&h.policy, &h.home, &items);
    assert!(report.refused.is_empty() && report.failed.is_empty(), "{report:?}");
    assert!(h.home.join(".toolbox/cache").is_dir(), "the cache folder itself stays");
    assert_eq!(fs::read_dir(h.home.join(".toolbox/cache")).unwrap().count(), 0);
    assert!(h.home.join(".toolbox/settings.json").exists());
}

// --- duplicates ---------------------------------------------------------------

fn write_pattern(path: &Path, len: usize, seed: u8, flip_at: Option<usize>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect();
    if let Some(i) = flip_at {
        data[i] ^= 0xFF;
    }
    fs::write(path, data).unwrap();
}

#[test]
fn finds_identical_files_and_only_those() {
    let h = home();
    let len = 400_000;
    write_pattern(&h.home.join("a.bin"), len, 1, None);
    write_pattern(&h.home.join("deep/er/copy-of-a.bin"), len, 1, None);
    write_pattern(&h.home.join("zz/another-copy.bin"), len, 1, None);
    // Same length, same first and last 64 KiB, one byte different in the middle.
    write_pattern(&h.home.join("almost.bin"), len, 1, Some(len / 2));
    // Same length, different throughout.
    write_pattern(&h.home.join("different.bin"), len, 2, None);
    write_pattern(&h.home.join("other-size.bin"), len + 1, 1, None);
    // A second, smaller pair that fits inside the sampled ends.
    write_pattern(&h.home.join("small1.bin"), 100_000, 7, None);
    write_pattern(&h.home.join("small2.bin"), 100_000, 7, None);
    write_pattern(&h.home.join("small-almost.bin"), 100_000, 7, Some(50_000));
    #[cfg(unix)]
    fs::hard_link(h.home.join("a.bin"), h.home.join("hard-link-to-a.bin")).unwrap();

    let t = scan_small(&h.home);
    let ctl = DuplicateControl::new();
    let groups = find_duplicates(&t, &h.policy, &ctl);
    let names = |g: &strata_reclaim::DuplicateGroup| -> Vec<String> {
        g.files.iter().map(|f| f.path.strip_prefix(&h.home).unwrap().display().to_string()).collect()
    };
    assert_eq!(groups.len(), 2, "{groups:?}");
    let big = names(&groups[0]);
    assert_eq!(big.len(), 3, "{big:?}");
    assert!(big[0] == "a.bin" || big[0] == "hard-link-to-a.bin", "shortest path is the one to keep: {big:?}");
    assert_eq!(&big[1..], ["zz/another-copy.bin", "deep/er/copy-of-a.bin"]);
    assert_eq!(groups[0].reclaimable_bytes(), 2 * groups[0].alloc_bytes);
    assert_eq!(names(&groups[1]), ["small1.bin", "small2.bin"]);
    assert!(groups.iter().all(|g| g.files.iter().all(|f| f.removable)));
    assert!(ctl.bytes_read() > 0);
}

#[test]
fn a_protected_copy_is_kept_and_never_offered() {
    let mut h = home();
    write_pattern(&h.home.join("vault/original.bin"), 300_000, 5, None);
    write_pattern(&h.home.join("Downloads/copy.bin"), 300_000, 5, None);
    write_pattern(&h.home.join("vault2/x.bin"), 200_000, 9, None);
    write_pattern(&h.home.join("vault/y.bin"), 200_000, 9, None);
    h.policy.protect(&h.home.join("vault").to_string_lossy()).unwrap();
    h.policy.protect(&h.home.join("vault2").to_string_lossy()).unwrap();

    let groups = find_duplicates(&scan_small(&h.home), &h.policy, &DuplicateControl::new());
    assert_eq!(groups.len(), 1, "a pair whose copies are both protected is not reported");
    let g = &groups[0];
    assert_eq!(g.files[0].path, h.home.join("vault/original.bin"));
    assert!(!g.files[0].removable && g.files[1].removable);
    assert_eq!(g.reclaimable_bytes(), g.alloc_bytes);
}

#[test]
fn identical_files_inside_tools_and_apps_are_not_duplicates_to_clean() {
    let h = home();
    write_pattern(&h.home.join("proj/node_modules/a/lib.bin"), 300_000, 6, None);
    write_pattern(&h.home.join("proj/node_modules/b/lib.bin"), 300_000, 6, None);
    write_pattern(&h.home.join("tools/engine.dll"), 300_000, 8, None);
    write_pattern(&h.home.join("tools/copy/engine.dll"), 300_000, 8, None);
    assert!(find_duplicates(&scan_small(&h.home), &h.policy, &DuplicateControl::new()).is_empty());
}

#[test]
fn files_that_changed_after_the_scan_are_left_out() {
    let h = home();
    write_pattern(&h.home.join("one.bin"), 300_000, 4, None);
    write_pattern(&h.home.join("two.bin"), 300_000, 4, None);
    write_pattern(&h.home.join("three.bin"), 300_000, 4, None);
    let t = scan_small(&h.home);
    fs::write(h.home.join("two.bin"), b"now something else").unwrap();
    fs::remove_file(h.home.join("three.bin")).unwrap();
    assert!(find_duplicates(&t, &h.policy, &DuplicateControl::new()).is_empty());
}

#[test]
fn a_cancelled_search_reports_nothing() {
    let h = home();
    write_pattern(&h.home.join("one.bin"), 300_000, 4, None);
    write_pattern(&h.home.join("two.bin"), 300_000, 4, None);
    let ctl = DuplicateControl::new();
    ctl.cancel();
    assert!(find_duplicates(&scan_small(&h.home), &h.policy, &ctl).is_empty());
}
