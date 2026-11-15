use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use strata_classify::{find_dirs, Attribution, Env, Os};
use strata_core::ScanTree;
use strata_safety::Policy;

/// The heading a suggestion is shown under, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// A cache that a known app keeps and rebuilds.
    AppCache,
    /// Folders that developer tools recreate on the next build or install.
    DeveloperJunk,
    /// Other cache and log folders.
    Cache,
    /// An installer in Downloads that has sat there a while.
    OldInstaller,
    /// A large file in Downloads untouched for months.
    OldDownload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Everything inside this folder. The folder itself stays.
    Contents(PathBuf),
    /// This one file or folder.
    Item(PathBuf),
}

impl Target {
    pub fn path(&self) -> &Path {
        match self {
            Target::Contents(p) | Target::Item(p) => p,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub group: Group,
    pub target: Target,
    /// Bytes on disk, as of the scan.
    pub bytes: u64,
    /// The app it belongs to, when a rule says so.
    pub app: Option<String>,
    /// True when its owner recreates it on its own; false when removing it
    /// loses something (a download, an installer).
    pub rebuildable: bool,
    /// One plain sentence for the person deciding.
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// "Now", for judging age. A parameter so results can be reproduced.
    pub now: SystemTime,
    /// Smaller finds are not worth a line on screen.
    pub min_bytes: u64,
    pub installer_age_days: u64,
    pub download_age_days: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            now: SystemTime::now(),
            min_bytes: 10 * 1024 * 1024,
            installer_age_days: 14,
            download_age_days: 90,
        }
    }
}

/// Folders, by name, that a tool recreates. `needs` names a file that must
/// sit beside the folder for the claim to hold (a `target` folder is only
/// Rust build output next to a `Cargo.toml`).
struct Junk {
    name: &'static str,
    needs: Option<&'static str>,
    reason: &'static str,
}

const JUNK: &[Junk] = &[
    Junk { name: "node_modules", needs: None, reason: "Installed JavaScript packages. Running the project's install command restores them." },
    Junk { name: "target", needs: Some("Cargo.toml"), reason: "Rust build output. The next build recreates it." },
    Junk { name: "Pods", needs: Some("Podfile"), reason: "Installed CocoaPods. Running pod install restores them." },
    Junk { name: ".next", needs: Some("package.json"), reason: "Next.js build output. The next build recreates it." },
    Junk { name: ".turbo", needs: None, reason: "Turborepo cache. It is rebuilt as needed." },
    Junk { name: ".parcel-cache", needs: None, reason: "Parcel build cache. It is rebuilt as needed." },
    Junk { name: ".gradle", needs: Some("build.gradle"), reason: "Gradle project cache. The next build recreates it." },
];

/// Tool caches at fixed places in the home folder: (pattern, reason).
const TOOL_CACHES: &[(&str, &str)] = &[
    ("~/.gradle/caches", "Gradle's download cache. Builds fetch what they need again."),
    ("~/.cargo/registry", "Rust's package cache. Builds fetch what they need again."),
    ("~/.m2/repository", "Maven's package cache. Builds fetch what they need again."),
    ("~/.npm/_cacache", "npm's download cache. Installs fetch what they need again."),
    ("~/.nuget/packages", "NuGet's package cache. Builds fetch what they need again."),
    ("~/.pub-cache", "Dart's package cache. Builds fetch what they need again."),
    ("~/.cocoapods/repos", "CocoaPods' spec cache. It is downloaded again when needed."),
];

/// Folders whose every child is a cache or log of some app.
fn cache_roots(os: Os) -> &'static [&'static str] {
    match os {
        Os::MacOs => &["~/Library/Caches/*", "~/Library/Logs/*"],
        Os::Windows => &["~/AppData/Local/CrashDumps"],
        Os::Linux => &["~/.cache/*"],
    }
}

const INSTALLER_EXTENSIONS: &[&str] = &["dmg", "pkg", "mpkg", "exe", "msi", "msix", "msixbundle", "appx", "iso", "deb", "rpm", "appimage"];

/// Name what could be cleaned up. Reads the scan result, plus a handful of
/// file dates and marker files from disk. Removes nothing.
pub fn suggest(tree: &ScanTree, attribution: &Attribution, env: &Env, policy: &Policy, opts: &Options) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    // Folders already covered, so nothing is offered twice or counted twice.
    let mut taken: Vec<u32> = Vec::new();

    let is_under = |mut dir: u32, top: u32| loop {
        if dir == top {
            return true;
        }
        match tree.dirs[dir as usize].parent {
            Some(p) => dir = p,
            None => return false,
        }
    };
    let overlaps = |taken: &[u32], dir: u32| taken.iter().any(|&t| is_under(dir, t) || is_under(t, dir));
    // The policy must allow what would actually be moved: the folder for an
    // item, something inside the folder for its contents.
    let allowed = |target: &Target| match target {
        Target::Item(p) => policy.check(p).is_ok(),
        Target::Contents(p) => policy.check(&p.join("item")).is_ok(),
    };

    // 1. Caches that app rules name.
    for &dir in &attribution.cache_dirs {
        let bytes = tree.dirs[dir as usize].alloc_bytes;
        let target = Target::Contents(tree.dir_path(dir));
        if bytes < opts.min_bytes || overlaps(&taken, dir) || !allowed(&target) {
            continue;
        }
        let app = attribution.owners[dir as usize].map(|o| attribution.apps[o.app as usize].name.clone());
        let reason = match &app {
            Some(name) => format!("Cache kept by {name}. It rebuilds what it needs."),
            None => "App cache. It is rebuilt as needed.".to_string(),
        };
        taken.push(dir);
        out.push(Suggestion { group: Group::AppCache, target, bytes, app, rebuildable: true, reason });
    }

    // 2. Tool caches at known places.
    for (pattern, reason) in TOOL_CACHES {
        for dir in find_dirs(tree, env, pattern) {
            let bytes = tree.dirs[dir as usize].alloc_bytes;
            let target = Target::Contents(tree.dir_path(dir));
            if bytes < opts.min_bytes || overlaps(&taken, dir) || !allowed(&target) {
                continue;
            }
            taken.push(dir);
            out.push(Suggestion {
                group: Group::DeveloperJunk,
                target,
                bytes,
                app: None,
                rebuildable: true,
                reason: reason.to_string(),
            });
        }
    }

    // 3. Rebuildable project folders, wherever they are. Folders are stored
    //    parent-first, so the outermost match is seen first and claims its subtree.
    for (id, dir) in tree.dirs.iter().enumerate() {
        let id = id as u32;
        if dir.alloc_bytes < opts.min_bytes || dir.flags.not_scanned() {
            continue;
        }
        let Some(junk) = JUNK.iter().find(|j| dir.name == j.name) else { continue };
        if overlaps(&taken, id) {
            continue;
        }
        let path = tree.dir_path(id);
        if let Some(marker) = junk.needs {
            if !path.parent().is_some_and(|p| p.join(marker).is_file()) {
                continue;
            }
        }
        let target = Target::Item(path);
        if !allowed(&target) {
            continue;
        }
        taken.push(id);
        out.push(Suggestion {
            group: Group::DeveloperJunk,
            target,
            bytes: dir.alloc_bytes,
            app: None,
            rebuildable: true,
            reason: junk.reason.to_string(),
        });
    }

    // 4. Whatever else sits in the system's cache and log folders.
    for pattern in cache_roots(env.os) {
        for dir in find_dirs(tree, env, pattern) {
            let bytes = tree.dirs[dir as usize].alloc_bytes;
            let target = Target::Item(tree.dir_path(dir));
            if bytes < opts.min_bytes || overlaps(&taken, dir) || !allowed(&target) {
                continue;
            }
            taken.push(dir);
            out.push(Suggestion {
                group: Group::Cache,
                target,
                bytes,
                app: None,
                rebuildable: true,
                reason: "Cache or log folder. Apps recreate what they need.".to_string(),
            });
        }
    }

    // 5. Large files that have sat in Downloads.
    let downloads = find_dirs(tree, env, "~/Downloads");
    for file in &tree.files {
        if file.alloc_bytes < opts.min_bytes
            || !downloads.iter().any(|&d| is_under(file.dir, d))
            || taken.iter().any(|&t| is_under(file.dir, t))
        {
            continue;
        }
        let path = tree.file_path(file);
        // Age comes from the file as it is now; one that is gone is skipped.
        let Some(age_days) = age_in_days(&path, opts.now) else { continue };
        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let (group, reason) = if INSTALLER_EXTENSIONS.contains(&extension.as_str()) {
            if age_days < opts.installer_age_days {
                continue;
            }
            (Group::OldInstaller, format!("Installer downloaded {age_days} days ago. Not needed once the app is installed."))
        } else if age_days >= opts.download_age_days {
            (Group::OldDownload, format!("In Downloads, unchanged for {age_days} days."))
        } else {
            continue;
        };
        let target = Target::Item(path);
        if !allowed(&target) {
            continue;
        }
        out.push(Suggestion { group, target, bytes: file.alloc_bytes, app: None, rebuildable: false, reason });
    }

    out.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then_with(|| b.bytes.cmp(&a.bytes))
            .then_with(|| a.target.path().cmp(b.target.path()))
    });
    out
}

fn age_in_days(path: &Path, now: SystemTime) -> Option<u64> {
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
    Some(age.as_secs() / 86_400)
}

/// The paths a suggestion stands for right now: the item itself, or what is
/// in the folder at this moment. Hand the result to `strata_safety::trash`.
pub fn expand(target: &Target) -> io::Result<Vec<PathBuf>> {
    match target {
        Target::Item(path) => Ok(vec![path.clone()]),
        Target::Contents(folder) => {
            let mut items = Vec::new();
            for entry in std::fs::read_dir(folder)? {
                items.push(entry?.path());
            }
            items.sort();
            Ok(items)
        }
    }
}
