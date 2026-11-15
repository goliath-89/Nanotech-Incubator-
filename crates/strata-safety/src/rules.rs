//! The protected-path lists. This file is the whole policy: what Strata may
//! never move to the Trash, and the few places inside protected areas whose
//! contents it may.
//!
//! Pattern syntax: `/`-separated; a part ending in `*` matches any name with
//! that prefix (`*` alone matches any name); `~` stands for every user's home
//! folder; on Windows `*:` means every drive.
//!
//! * `Tree`  - the path and everything inside it is protected.
//! * `Exact` - the path itself is protected; its contents are not.
//! * `Open`  - a hole in a protected tree: the folder stays, its contents may go.
//!
//! When several rules match, the longest one decides. Anything that *contains*
//! a path named here is protected as well, whatever the rule's kind.
//!
//! Open rules can only be added here, in code review. Nothing at run time,
//! including the app-rule files, can open a protected area.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Open,
    Exact,
    Tree,
}
use Kind::{Exact, Open, Tree};

pub(crate) const MACOS: &[(&str, Kind)] = &[
    // The operating system.
    ("/System", Tree),
    ("/Library", Tree),
    ("/Applications", Tree),
    ("/bin", Tree),
    ("/sbin", Tree),
    ("/usr", Tree),
    ("/etc", Tree),
    ("/var", Tree),
    ("/tmp", Tree),
    ("/private", Tree),
    ("/dev", Tree),
    ("/cores", Tree),
    ("/opt", Tree),
    ("/home", Tree),
    ("/Network", Tree),
    ("/.vol", Tree),
    // Other volumes: the mount folder and each volume's root stay.
    ("/Volumes", Exact),
    ("/Volumes/*", Exact),
    // Bookkeeping folders macOS keeps on every volume.
    ("/.Spotlight-V100", Tree),
    ("/.fseventsd", Tree),
    ("/.Trashes", Tree),
    ("/.DocumentRevisions-V100", Tree),
    ("/.TemporaryItems", Tree),
    ("/Volumes/*/.Spotlight-V100", Tree),
    ("/Volumes/*/.fseventsd", Tree),
    ("/Volumes/*/.Trashes", Tree),
    ("/Volumes/*/.DocumentRevisions-V100", Tree),
    ("/Volumes/*/.TemporaryItems", Tree),
    // Users.
    ("/Users", Exact),
    ("/Users/Shared", Exact),
    ("~", Exact),
    ("~/Desktop", Exact),
    ("~/Documents", Exact),
    ("~/Downloads", Exact),
    ("~/Movies", Exact),
    ("~/Music", Exact),
    ("~/Pictures", Exact),
    ("~/Public", Exact),
    ("~/Sites", Exact),
    ("~/Applications", Tree),
    ("~/Library", Tree),
    // Hidden entries at the top of a home folder: shell and tool settings.
    // The entry stays; what is inside a hidden folder (caches mostly) may go.
    ("~/.*", Exact),
    ("~/.Trash", Tree),
    ("~/.ssh", Tree),
    ("~/.gnupg", Tree),
    ("~/.aws", Tree),
    ("~/.kube", Tree),
    ("~/.password-store", Tree),
    // Holes: caches and logs that their owners rebuild.
    ("/Library/Caches", Open),
    ("/Library/Logs", Open),
    ("~/Library/Caches", Open),
    ("~/Library/Logs", Open),
    ("~/Library/Developer/Xcode/DerivedData", Open),
    ("~/Library/Developer/Xcode/iOS DeviceSupport", Open),
    ("~/Library/Developer/CoreSimulator/Caches", Open),
    ("~/Library/Containers/*/Data/Library/Caches", Open),
    ("~/Library/Group Containers/*/Library/Caches", Open),
    ("~/Library/Application Support/*/Cache", Open),
    ("~/Library/Application Support/*/Code Cache", Open),
    ("~/Library/Application Support/*/GPUCache", Open),
    ("~/Library/Application Support/*/CachedData", Open),
];

pub(crate) const WINDOWS: &[(&str, Kind)] = &[
    // The operating system, on whichever drive it sits.
    ("*:/Windows", Tree),
    ("*:/Windows.old", Tree),
    ("*:/Program Files", Tree),
    ("*:/Program Files (x86)", Tree),
    ("*:/ProgramData", Tree),
    ("*:/Documents and Settings", Tree),
    ("*:/System Volume Information", Tree),
    ("*:/Recovery", Tree),
    ("*:/Boot", Tree),
    ("*:/EFI", Tree),
    ("*:/PerfLogs", Tree),
    ("*:/MSOCache", Tree),
    ("*:/Config.Msi", Tree),
    ("*:/inetpub", Tree),
    ("*:/ESD", Tree),
    ("*:/OneDriveTemp", Tree),
    // `$Recycle.Bin`, `$WinREAgent`, `$Extend` and the like.
    ("*:/$*", Tree),
    ("*:/pagefile.sys", Exact),
    ("*:/hiberfil.sys", Exact),
    ("*:/swapfile.sys", Exact),
    ("*:/bootmgr", Exact),
    ("*:/BOOTNXT", Exact),
    // Users.
    ("*:/Users", Exact),
    ("*:/Users/Public", Exact),
    ("*:/Users/Default", Tree),
    ("*:/Users/All Users", Tree),
    ("~", Exact),
    ("~/Desktop", Exact),
    ("~/Documents", Exact),
    ("~/Downloads", Exact),
    ("~/Music", Exact),
    ("~/Pictures", Exact),
    ("~/Videos", Exact),
    ("~/Saved Games", Exact),
    ("~/Favorites", Exact),
    ("~/Links", Exact),
    ("~/Contacts", Exact),
    ("~/Searches", Exact),
    ("~/3D Objects", Exact),
    ("~/OneDrive*", Exact),
    ("~/AppData", Tree),
    // The user's registry hive and its logs.
    ("~/ntuser*", Exact),
    ("~/.*", Exact),
    ("~/.ssh", Tree),
    ("~/.gnupg", Tree),
    ("~/.aws", Tree),
    ("~/.kube", Tree),
    // Holes: caches and temporary files that their owners rebuild.
    ("*:/Windows/Temp", Open),
    ("*:/Windows/SoftwareDistribution/Download", Open),
    ("~/AppData/Local/Temp", Open),
    ("~/AppData/Local/CrashDumps", Open),
    ("~/AppData/Local/Microsoft/Windows/INetCache", Open),
    ("~/AppData/Local/Microsoft/Windows/Explorer", Open),
    ("~/AppData/Local/Google/Chrome/User Data/*/Cache", Open),
    ("~/AppData/Local/Google/Chrome/User Data/*/Code Cache", Open),
    ("~/AppData/Local/Microsoft/Edge/User Data/*/Cache", Open),
    ("~/AppData/Local/Microsoft/Edge/User Data/*/Code Cache", Open),
    ("~/AppData/Local/Mozilla/Firefox/Profiles/*/cache2", Open),
    ("~/AppData/Local/npm-cache", Open),
    ("~/AppData/Local/pip/Cache", Open),
    ("~/AppData/Local/NuGet/v3-cache", Open),
    ("~/AppData/Local/*/Cache", Open),
    ("~/AppData/Roaming/*/Cache", Open),
    ("~/AppData/Roaming/*/Code Cache", Open),
    ("~/AppData/Roaming/*/GPUCache", Open),
    ("~/AppData/Roaming/*/CachedData", Open),
];

/// Linux is not a shipping platform; this keeps development machines safe.
pub(crate) const LINUX: &[(&str, Kind)] = &[
    ("/bin", Tree),
    ("/boot", Tree),
    ("/dev", Tree),
    ("/etc", Tree),
    ("/lib", Tree),
    ("/lib32", Tree),
    ("/lib64", Tree),
    ("/libx32", Tree),
    ("/proc", Tree),
    ("/run", Tree),
    ("/sbin", Tree),
    ("/sys", Tree),
    ("/usr", Tree),
    ("/var", Tree),
    ("/snap", Tree),
    ("/opt", Tree),
    ("/srv", Tree),
    ("/nix", Tree),
    ("/lost+found", Tree),
    ("/home", Exact),
    ("/mnt", Exact),
    ("/mnt/*", Exact),
    ("/media", Exact),
    ("/media/*", Exact),
    ("/media/*/*", Exact),
    ("/tmp", Exact),
    ("~", Exact),
    ("~/Desktop", Exact),
    ("~/Documents", Exact),
    ("~/Downloads", Exact),
    ("~/Music", Exact),
    ("~/Pictures", Exact),
    ("~/Videos", Exact),
    ("~/.*", Exact),
    ("~/.local/share", Exact),
    ("~/.local/share/Trash", Tree),
    ("~/.ssh", Tree),
    ("~/.gnupg", Tree),
    ("~/.aws", Tree),
    ("~/.kube", Tree),
    ("~/.password-store", Tree),
    ("/var/tmp", Open),
];

/// Where home folders live. `~` in a rule expands to each of these, and to
/// the current user's real home folder if that is somewhere else.
pub(crate) const MACOS_HOMES: &[&str] = &["/Users/*"];
pub(crate) const WINDOWS_HOMES: &[&str] = &["*:/Users/*"];
pub(crate) const LINUX_HOMES: &[&str] = &["/home/*", "/root"];

/// macOS folders that are really programs. Neither they nor anything inside
/// them may be moved, wherever they are.
pub(crate) const MACOS_PROGRAM_SUFFIXES: &[&str] = &[
    ".app", ".framework", ".kext", ".appex", ".xpc", ".bundle", ".plugin", ".prefpane",
    ".systemextension", ".dext",
];

/// macOS folders that are really one document or library. The whole thing is
/// the user's to move; pulling pieces out of it would corrupt it.
pub(crate) const MACOS_PACKAGE_SUFFIXES: &[&str] = &[
    ".photoslibrary", ".fcpbundle", ".musiclibrary", ".tvlibrary", ".imovielibrary", ".aplibrary",
    ".sparsebundle", ".logicx", ".band", ".photolibrary", ".lrdata", ".xcodeproj", ".xcworkspace",
    ".rtfd", ".pages", ".numbers", ".key", ".vmwarevm", ".pvm", ".utm",
];
