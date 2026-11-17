//! What a folder's place in the tree says about the files inside it.
//!
//! A `.png` inside an app is part of the app; a `.mp4` in a cache folder is
//! cache. So before looking at file names, look at where the folder is.

use std::path::{Component, Path};
use strata_core::Kind;

/// Folder names that mark everything below them, wherever they appear.
/// Compared in lower case.
const CACHE_NAMES: &[&str] = &[
    "caches",
    "cache",
    ".cache",
    "code cache",
    "gpucache",
    "cacheddata",
    "cache_data",
    "cachestorage",
    "shadercache",
    "temp",
    "tmp",
    "inetcache",
    "crashdumps",
    "crashpad",
    "crash reports",
    "temporary internet files",
    "service worker",
    "blob_storage",
];
const DEVELOPER_NAMES: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "deriveddata",
    "coresimulator",
    "xcuserdata",
    ".gradle",
    ".m2",
    ".ivy2",
    ".sbt",
    ".cargo",
    ".rustup",
    ".npm",
    ".pnpm-store",
    ".yarn",
    ".nvm",
    ".pyenv",
    ".rbenv",
    ".rvm",
    ".virtualenvs",
    "venv",
    ".venv",
    "__pycache__",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".docker",
    "com.docker.docker",
    ".android",
    ".cocoapods",
    ".nuget",
    ".pub-cache",
    ".terraform",
    ".vagrant",
    ".conda",
    "anaconda3",
    "miniconda3",
    "site-packages",
    ".ipynb_checkpoints",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
    "bower_components",
    ".dart_tool",
    ".vs",
    ".idea",
    "homebrew",
    "cellar",
    ".local-chromium",
];
const GAME_NAMES: &[&str] = &[
    "steamapps",
    "steamlibrary",
    "epic games",
    "gog games",
    "gog galaxy",
    "xboxgames",
    "riot games",
    "battle.net",
    "origin games",
    "ea games",
    "ubisoft game launcher",
    "rockstar games",
];
const APP_NAMES: &[&str] = &[
    "applications",
    "program files",
    "program files (x86)",
    "windowsapps",
];

/// Top-level folders that belong to the operating system.
const SYSTEM_TOP: &[&str] = &[
    // macOS
    "system",
    "library",
    "private",
    "usr",
    "bin",
    "sbin",
    "etc",
    "var",
    "cores",
    "opt",
    // Windows
    "windows",
    "windows.old",
    "programdata",
    "system volume information",
    "recovery",
    "$winreagent",
    "$windows.~bt",
    "$windows.~ws",
    "perflogs",
    "msocache",
    "boot",
    "efi",
    // Linux
    "lib",
    "lib32",
    "lib64",
    "libx32",
    "proc",
    "sys",
    "run",
    "snap",
    "srv",
    "nix",
];

/// Cache names so ordinary that people use them for their own folders too.
/// They are not trusted inside a personal folder.
const WEAK_CACHE_NAMES: &[&str] = &["temp", "tmp", "cache", "caches"];

/// Folders directly inside a home folder that hold a person's own files.
const PERSONAL_NAMES: &[&str] = &[
    "documents",
    "desktop",
    "downloads",
    "pictures",
    "movies",
    "music",
    "videos",
    "public",
];

/// The kind shared by every file under `dir`, if its location settles it.
/// `dir` must be an absolute path.
pub fn folder_kind(dir: &Path) -> Option<Kind> {
    let mut found: Option<Kind> = None;
    let mut depth = 0usize;
    let mut previous = String::new();
    // `/Users/<name>/...`, `C:\Users\<name>\...` or `/home/<name>/...`.
    let mut under_homes = false;
    // Inside Documents, Pictures and the like: the person's own material.
    let mut personal = false;
    let mut previous_top = String::new();
    for component in dir.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        let name = name.to_string_lossy().to_lowercase();
        let name = name.as_str();
        if depth == 0 {
            under_homes = name == "users" || name == "home";
        }
        let home_level = under_homes && depth == 2;
        if home_level && (PERSONAL_NAMES.contains(&name) || name.starts_with("onedrive")) {
            personal = true;
        }

        // The Recycle Bin and Trash: at the top of a drive on Windows and
        // on macOS volumes, in each home folder on macOS and Linux.
        let trash = (depth == 0 && matches!(name, "$recycle.bin" | ".trashes"))
            || (home_level && name == ".trash")
            || (under_homes && depth == 4 && name == "trash" && previous == "share")
            || (depth == 2 && previous_top == "volumes" && name == ".trashes");
        let here = if trash {
            Some(Kind::Trash)
        } else if GAME_NAMES.contains(&name) {
            Some(Kind::Games)
        } else if CACHE_NAMES.contains(&name) && !(personal && WEAK_CACHE_NAMES.contains(&name)) {
            Some(Kind::Caches)
        } else if DEVELOPER_NAMES.contains(&name) || (name == "developer" && previous == "library")
        {
            Some(Kind::Developer)
        } else if name.ends_with(".app") {
            Some(Kind::Apps)
        } else if APP_NAMES.contains(&name) && (depth == 0 || home_level) {
            // Only the real programs folders: at the top of a drive or of a
            // home folder. A "Applications" folder in Documents is paperwork.
            Some(Kind::Apps)
        } else if depth == 0 && SYSTEM_TOP.contains(&name) {
            Some(Kind::System)
        } else {
            None
        };
        match (found, here) {
            // Thrown-away files are trash whatever they were.
            (_, Some(Kind::Trash)) => found = Some(Kind::Trash),
            // The first marker on the way down decides...
            (None, Some(kind)) => found = Some(kind),
            // ...except that a game library inside a programs folder is games,
            // and caches inside the system's or an app's folders are caches.
            (Some(Kind::Apps), Some(Kind::Games)) => found = Some(Kind::Games),
            (Some(Kind::System), Some(Kind::Caches)) => found = Some(Kind::Caches),
            // Developer tools installed under a system folder (`/opt/homebrew`).
            (Some(Kind::System), Some(Kind::Developer)) => found = Some(Kind::Developer),
            _ => {}
        }
        if depth == 0 {
            previous_top = name.to_string();
        }
        depth += 1;
        previous = name.to_string();
    }
    found
}
