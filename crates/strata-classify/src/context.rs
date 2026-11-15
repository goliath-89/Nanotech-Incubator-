//! What a folder's place in the tree says about the files inside it.
//!
//! A `.png` inside an app is part of the app; a `.mp4` in a cache folder is
//! cache. So before looking at file names, look at where the folder is.

use std::path::{Component, Path};
use strata_core::Kind;

/// Folder names that mark everything below them, wherever they appear.
/// Compared in lower case.
const CACHE_NAMES: &[&str] = &[
    "caches", "cache", ".cache", "code cache", "gpucache", "cacheddata", "cache_data", "cachestorage",
    "shadercache", "temp", "tmp", "inetcache", "crashdumps", "crashpad", "crash reports",
    "temporary internet files", "service worker", "blob_storage",
];
const DEVELOPER_NAMES: &[&str] = &[
    "node_modules", ".git", ".svn", ".hg", "deriveddata", "coresimulator", "xcuserdata", ".gradle", ".m2",
    ".ivy2", ".sbt", ".cargo", ".rustup", ".npm", ".pnpm-store", ".yarn", ".nvm", ".pyenv", ".rbenv",
    ".rvm", ".virtualenvs", "venv", ".venv", "__pycache__", ".tox", ".mypy_cache", ".pytest_cache",
    ".docker", "com.docker.docker", ".android", ".cocoapods", "pods", ".nuget", ".pub-cache",
    ".terraform", ".vagrant", ".conda", "anaconda3", "miniconda3", "site-packages", ".ipynb_checkpoints",
    ".next", ".nuxt", ".turbo", ".parcel-cache", "bower_components", ".dart_tool", ".vs", ".idea",
    "homebrew", "cellar", "wsl", ".local-chromium",
];
const GAME_NAMES: &[&str] = &[
    "steamapps", "steamlibrary", "epic games", "gog games", "gog galaxy", "xboxgames", "riot games",
    "battle.net", "origin games", "ea games", "ubisoft game launcher", "rockstar games", "itch",
];
const APP_NAMES: &[&str] = &["applications", "program files", "program files (x86)", "windowsapps"];

/// Top-level folders that belong to the operating system.
const SYSTEM_TOP: &[&str] = &[
    // macOS
    "system", "library", "private", "usr", "bin", "sbin", "etc", "var", "cores", "opt",
    // Windows
    "windows", "windows.old", "programdata", "$recycle.bin", "system volume information", "recovery",
    "$winreagent", "$windows.~bt", "$windows.~ws", "perflogs", "msocache", "boot", "efi",
    // Linux
    "lib", "lib32", "lib64", "libx32", "proc", "sys", "run", "snap", "srv", "nix",
];

/// The kind shared by every file under `dir`, if its location settles it.
/// `dir` must be an absolute path.
pub fn folder_kind(dir: &Path) -> Option<Kind> {
    let mut found: Option<Kind> = None;
    let mut depth = 0usize;
    let mut previous = String::new();
    for component in dir.components() {
        let Component::Normal(name) = component else { continue };
        let name = name.to_string_lossy().to_lowercase();
        let here = if GAME_NAMES.contains(&name.as_str()) {
            Some(Kind::Games)
        } else if CACHE_NAMES.contains(&name.as_str()) {
            Some(Kind::Caches)
        } else if DEVELOPER_NAMES.contains(&name.as_str()) || (name == "developer" && previous == "library") {
            Some(Kind::Developer)
        } else if name.ends_with(".app") || APP_NAMES.contains(&name.as_str()) {
            Some(Kind::Apps)
        } else if depth == 0 && SYSTEM_TOP.contains(&name.as_str()) {
            Some(Kind::System)
        } else {
            None
        };
        match (found, here) {
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
        depth += 1;
        previous = name;
    }
    found
}
