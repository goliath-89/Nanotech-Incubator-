//! Policy tests. They run the macOS and Windows rules on any machine, since
//! the policy only looks at the text of a path.

use crate::rules::{self, Kind};
use crate::{Platform, Policy, Reason};
use proptest::prelude::*;

fn mac() -> Policy {
    Policy::new(Platform::MacOs, &[]).unwrap()
}
fn win() -> Policy {
    Policy::new(Platform::Windows, &[]).unwrap()
}
fn linux() -> Policy {
    Policy::new(Platform::Linux, &[]).unwrap()
}

fn reason(policy: &Policy, path: &str) -> Option<Reason> {
    policy.check_str(path).err().map(|r| r.reason)
}

#[track_caller]
fn refused(policy: &Policy, paths: &[&str]) {
    for p in paths {
        assert!(policy.check_str(p).is_err(), "must be refused: {p}");
    }
}

#[track_caller]
fn allowed(policy: &Policy, paths: &[&str]) {
    for p in paths {
        if let Err(r) = policy.check_str(p) {
            panic!("must be allowed: {p} (refused: {r})");
        }
    }
}

#[test]
fn macos_protects_the_system_and_the_folders_that_hold_user_files() {
    refused(
        &mac(),
        &[
            "/",
            "/System",
            "/System/Library/CoreServices/Finder.app",
            "/Library",
            "/Library/Preferences/com.apple.loginwindow.plist",
            "/Applications",
            "/Applications/Safari.app",
            "/Applications/Xcode.app/Contents/Developer",
            "/usr/local/bin/brew",
            "/opt/homebrew/Cellar",
            "/private/var/db",
            "/Users",
            "/Users/Shared",
            "/Users/sam",
            "/Users/sam/Documents",
            "/Users/sam/Desktop",
            "/Users/sam/Downloads",
            "/Users/sam/Pictures",
            "/Users/sam/Library",
            "/Users/sam/Library/Preferences/com.apple.finder.plist",
            "/Users/sam/Library/Keychains/login.keychain-db",
            "/Users/sam/Library/Mail/V10",
            "/Users/sam/Library/Mobile Documents/com~apple~CloudDocs/taxes.pdf",
            "/Users/sam/Library/Application Support/Steam",
            "/Users/sam/Library/Caches",
            "/Users/sam/Library/Containers/com.apple.mail/Data/Library/Caches",
            "/Users/sam/.zshrc",
            "/Users/sam/.ssh/id_ed25519",
            "/Users/sam/.Trash/old.zip",
            "/Users/sam/.npm",
            "/Volumes",
            "/Volumes/Backup",
            "/Volumes/Backup/.Spotlight-V100/Store-V2",
            "/somewhere",
        ],
    );
}

#[test]
fn macos_allows_ordinary_files_and_known_caches() {
    allowed(
        &mac(),
        &[
            "/Users/sam/Downloads/installer.dmg",
            "/Users/sam/Documents/old-project",
            "/Users/sam/Movies/holiday.mov",
            "/Users/sam/code/app/node_modules",
            "/Users/sam/.npm/_cacache",
            "/Users/sam/.gradle/caches",
            "/Users/sam/Library/Caches/com.apple.Safari",
            "/Users/sam/Library/Caches/Homebrew/downloads/x.tar.gz",
            "/Users/sam/Library/Logs/DiagnosticReports",
            "/Users/sam/Library/Developer/Xcode/DerivedData/App-abc",
            "/Users/sam/Library/Containers/com.apple.mail/Data/Library/Caches/x",
            "/Users/sam/Library/Application Support/Slack/Cache/data_1",
            "/Users/sam/Library/Application Support/Code/CachedData/abc",
            "/Library/Caches/com.apple.iconservices.store",
            "/Volumes/Backup/Old Movies",
            "/Users/sam/Pictures/Photos Library.photoslibrary",
        ],
    );
}

#[test]
fn macos_programs_and_package_internals_are_off_limits_everywhere() {
    let p = mac();
    assert_eq!(
        reason(&p, "/Users/sam/Downloads/Tool.app"),
        Some(Reason::InsideApplication)
    );
    assert_eq!(
        reason(&p, "/Users/sam/Downloads/Tool.app/Contents/MacOS/tool"),
        Some(Reason::InsideApplication)
    );
    assert_eq!(
        reason(&p, "/Users/sam/Library/Caches/x/Helper.framework/Versions"),
        Some(Reason::InsideApplication)
    );
    assert_eq!(
        reason(
            &p,
            "/Users/sam/Pictures/Photos Library.photoslibrary/originals/0/a.heic"
        ),
        Some(Reason::InsidePackage)
    );
    assert_eq!(
        reason(&p, "/Users/sam/Movies/Trip.fcpbundle/Original Media"),
        Some(Reason::InsidePackage)
    );
    assert_eq!(
        reason(&p, "/Users/sam/Downloads/TOOL.APP"),
        Some(Reason::InsideApplication)
    );
}

#[test]
fn macos_ignores_case_and_lookalike_letters() {
    refused(
        &mac(),
        &[
            "/SYSTEM/Library",
            "/users/SAM/documents",
            "/Users/sam/LIBRARY/Preferences/x",
            "/U\u{17F}ers/sam",          // long s
            "/Users/sam/Des\u{212A}top", // Kelvin sign
            "/\u{FB01}le-is-not-here/../System",
            "/Sy\u{FB06}em/Library", // st ligature
        ],
    );
}

#[test]
fn windows_protects_the_system_and_the_folders_that_hold_user_files() {
    refused(
        &win(),
        &[
            r"C:\",
            r"C:\Windows",
            r"C:\Windows\System32\drivers\etc\hosts",
            r"C:\Windows\Temp",
            r"C:\Program Files\Git",
            r"C:\Program Files (x86)\Steam\steamapps",
            r"C:\ProgramData\Microsoft",
            r"C:\$Recycle.Bin\S-1-5-21",
            r"C:\$WinREAgent",
            r"C:\System Volume Information",
            r"C:\pagefile.sys",
            r"C:\hiberfil.sys",
            r"C:\Users",
            r"C:\Users\Public",
            r"C:\Users\Default\NTUSER.DAT",
            r"C:\Users\sam",
            r"C:\Users\sam\Documents",
            r"C:\Users\sam\Desktop",
            r"C:\Users\sam\OneDrive",
            r"C:\Users\sam\OneDrive - Contoso",
            r"C:\Users\sam\NTUSER.DAT",
            r"C:\Users\sam\ntuser.dat.LOG1",
            r"C:\Users\sam\AppData",
            r"C:\Users\sam\AppData\Roaming\Microsoft\Credentials",
            r"C:\Users\sam\AppData\Local\Google\Chrome\User Data\Default\Login Data",
            r"C:\Users\sam\AppData\Local\Temp",
            r"C:\Users\sam\.ssh\id_rsa",
            r"C:\Users\sam\.gitconfig",
            // The same folders on a second drive (another Windows install).
            r"D:\Windows\System32",
            r"E:\$RECYCLE.BIN",
            r"D:\",
        ],
    );
}

#[test]
fn windows_allows_ordinary_files_and_known_caches() {
    allowed(
        &win(),
        &[
            r"C:\Users\sam\Downloads\setup.exe",
            r"C:\Users\sam\Documents\old-project",
            r"C:\Users\sam\Videos\clip.mp4",
            r"C:\Users\sam\source\repos\app\node_modules",
            r"C:\Users\sam\AppData\Local\Temp\tmp1234",
            r"C:\Users\sam\AppData\Local\Google\Chrome\User Data\Default\Cache\data_1",
            r"C:\Users\sam\AppData\Local\Microsoft\Edge\User Data\Profile 1\Code Cache\js",
            r"C:\Users\sam\AppData\Roaming\Slack\Cache\f_0001",
            r"C:\Users\sam\AppData\Local\npm-cache\_cacache",
            r"C:\Users\sam\.nuget\packages",
            r"C:\Windows\Temp\old.log",
            r"C:\Windows\SoftwareDistribution\Download\abc",
            r"D:\Movies\film.mkv",
            r"D:\Games\Old Game",
            r"\\?\C:\Users\sam\Downloads\setup.exe",
            "C:/Users/sam/Downloads/setup.exe",
        ],
    );
}

#[test]
fn windows_refuses_every_spelling_trick() {
    let p = win();
    let cases: &[(&str, Reason)] = &[
        (r"c:\WINDOWS\system32", Reason::Protected),
        (r"C:/Windows/System32", Reason::Protected),
        (r"C:\\Windows\\\System32", Reason::Protected),
        (r"\\?\C:\Windows\System32", Reason::Protected),
        (r"C:\Windows.\System32", Reason::UnusualPath),
        (r"C:\Windows \System32", Reason::UnusualPath),
        (
            r"C:\Users\sam\Downloads\..\..\..\Windows",
            Reason::UnusualPath,
        ),
        (r"C:\Users\sam\Downloads\.\x", Reason::UnusualPath),
        (r"C:\PROGRA~1\Git", Reason::UnusualPath),
        (r"C:\Users\sam\DOCUME~1", Reason::UnusualPath),
        (
            r"C:\Users\sam\Downloads\file.txt:stream",
            Reason::UnusualPath,
        ),
        (r"C:\Users\sam\Downloads\a*b", Reason::UnusualPath),
        (r"C:Windows", Reason::NotAbsolute),
        (r"C:", Reason::NotAbsolute),
        (r"\Windows\System32", Reason::NotAbsolute),
        (r"Windows\System32", Reason::NotAbsolute),
        (r"\\server\share\folder", Reason::NetworkPath),
        (r"\\?\UNC\server\share\folder", Reason::NetworkPath),
        (r"\\.\PhysicalDrive0", Reason::NetworkPath),
        (
            r"\\?\Volume{b75e2c83-0000-0000-0000-602f00000000}\Windows",
            Reason::NotAbsolute,
        ),
        (r"C:\Users", Reason::ContainsProtected),
        (r"C:\", Reason::VolumeRoot),
        ("", Reason::NotAbsolute),
        ("C:\\Users\\sam\\Down\0loads\\x", Reason::UnusualPath),
    ];
    for (path, expected) in cases {
        assert_eq!(reason(&p, path), Some(*expected), "{path}");
    }
}

#[test]
fn unix_refuses_relative_and_dotted_paths() {
    for p in [mac(), linux()] {
        for path in [
            "",
            "Users/sam/x",
            "./x",
            "~/Downloads/x",
            "/Users/sam/../../System",
            "/Users/sam/./x",
            "/a/b/..",
            "/x\0/y",
        ] {
            assert!(p.check_str(path).is_err(), "{path:?}");
        }
    }
    assert_eq!(
        reason(&linux(), "/home/sam//Downloads///x"),
        None,
        "doubled slashes are harmless"
    );
}

#[test]
fn linux_is_case_sensitive_but_still_protects_its_system() {
    let p = linux();
    refused(
        &p,
        &[
            "/usr/bin/env",
            "/etc/passwd",
            "/home",
            "/home/sam",
            "/home/sam/.bashrc",
            "/root",
            "/root/.ssh/x",
            "/tmp",
            "/data",
        ],
    );
    allowed(
        &p,
        &[
            "/home/sam/Downloads/x",
            "/home/sam/.cache/pip",
            "/tmp/build-123",
            "/home/sam/USR",
            "/mnt/disk/old",
            "/var/tmp/x",
        ],
    );
}

#[test]
fn a_home_folder_in_an_unusual_place_is_protected_like_any_other() {
    let mac = Policy::new(Platform::MacOs, &["/Volumes/Data/Homes/sam"]).unwrap();
    refused(
        &mac,
        &[
            "/Volumes/Data/Homes/sam",
            "/Volumes/Data/Homes/sam/Documents",
            "/Volumes/Data/Homes/sam/Library/Preferences/x",
            "/Volumes/Data/Homes",
        ],
    );
    allowed(
        &mac,
        &[
            "/Volumes/Data/Homes/sam/Downloads/x",
            "/Volumes/Data/Homes/sam/Library/Caches/x",
        ],
    );

    let win = Policy::new(Platform::Windows, &[r"D:\Profiles\sam"]).unwrap();
    refused(
        &win,
        &[
            r"D:\Profiles\sam",
            r"D:\Profiles\sam\AppData\Roaming\x",
            r"D:\Profiles\sam\Documents",
            r"D:\Profiles",
        ],
    );
    allowed(
        &win,
        &[
            r"D:\Profiles\sam\Downloads\x",
            r"D:\Profiles\sam\AppData\Local\Temp\x",
        ],
    );

    assert!(Policy::new(Platform::MacOs, &["relative/home"]).is_err());
    assert!(Policy::new(Platform::Windows, &[r"C:\"]).is_err());
}

#[test]
fn protect_adds_a_folder_and_nothing_can_remove_one() {
    let mut p = linux();
    assert!(p.check_str("/home/sam/apps/strata/history.sqlite").is_ok());
    p.protect("/home/sam/apps/strata").unwrap();
    refused(
        &p,
        &[
            "/home/sam/apps/strata",
            "/home/sam/apps/strata/history.sqlite",
            "/home/sam/apps",
        ],
    );
    assert_eq!(p.protect("/"), Err(Reason::VolumeRoot));
    assert_eq!(p.protect("not/absolute"), Err(Reason::NotAbsolute));
}

#[test]
fn refusals_explain_themselves() {
    let r = mac()
        .check_str("/Users/sam/Library/Preferences/x")
        .unwrap_err();
    assert_eq!(r.rule.as_deref(), Some("/Users/*/Library"));
    assert!(r.to_string().contains("needed by the system"));
}

// ---------------------------------------------------------------------------
// Randomised checks. Each rule in the tables is turned into concrete paths
// and attacked with random suffixes, case changes and separators.
// ---------------------------------------------------------------------------

fn table(platform: Platform) -> (&'static [(&'static str, Kind)], &'static [&'static str]) {
    match platform {
        Platform::MacOs => (rules::MACOS, rules::MACOS_HOMES),
        Platform::Windows => (rules::WINDOWS, rules::WINDOWS_HOMES),
        Platform::Linux => (rules::LINUX, rules::LINUX_HOMES),
    }
}

/// Every rule as a list of pattern parts (drive first on Windows), with `~`
/// expanded. Written separately from the policy's own expansion on purpose.
fn patterns(platform: Platform) -> Vec<(Vec<String>, Kind)> {
    let (rules, homes) = table(platform);
    let mut out = Vec::new();
    for (pattern, kind) in rules {
        let expanded: Vec<String> = match pattern.strip_prefix('~') {
            Some(rest) => homes.iter().map(|h| format!("{h}{rest}")).collect(),
            None => vec![pattern.to_string()],
        };
        for e in expanded {
            out.push((
                e.split('/')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
                *kind,
            ));
        }
    }
    out
}

/// Fill a pattern's wildcards with `fill` to get a concrete path.
fn instantiate(parts: &[String], fill: &str, platform: Platform) -> String {
    let concrete: Vec<String> = parts
        .iter()
        .map(|p| match p.as_str() {
            "*:" => "C:".to_string(),
            _ => match p.strip_suffix('*') {
                Some(prefix) => format!("{prefix}{fill}"),
                None => p.clone(),
            },
        })
        .collect();
    match platform {
        Platform::Windows => format!("{}\\{}", concrete[0], concrete[1..].join("\\")),
        _ => format!("/{}", concrete.join("/")),
    }
}

fn pattern_part_matches(pattern: &str, name: &str, platform: Platform) -> bool {
    let (pattern, name) = if platform == Platform::Linux {
        (pattern.to_string(), name.to_string())
    } else {
        (pattern.to_lowercase(), name.to_lowercase())
    };
    if pattern == "*:" {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// Is some Open rule a strict prefix of `path_parts`?
fn inside_an_open_rule(path_parts: &[String], platform: Platform) -> bool {
    patterns(platform).iter().any(|(parts, kind)| {
        *kind == Kind::Open
            && parts.len() < path_parts.len()
            && parts
                .iter()
                .zip(path_parts)
                .all(|(p, n)| pattern_part_matches(p, n, platform))
    })
}

fn split(path: &str, platform: Platform) -> Vec<String> {
    let seps: &[char] = if platform == Platform::Windows {
        &['\\', '/']
    } else {
        &['/']
    };
    path.split(seps)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn any_platform() -> impl Strategy<Value = Platform> {
    prop_oneof![
        Just(Platform::MacOs),
        Just(Platform::Windows),
        Just(Platform::Linux)
    ]
}

/// Names a file could plausibly have, weighted towards troublesome ones.
fn name() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[a-zA-Z0-9][a-zA-Z0-9 _-]{0,10}[a-zA-Z0-9]",
        1 => Just("Cache".to_string()),
        1 => Just("Caches".to_string()),
        1 => Just("Temp".to_string()),
        1 => Just("Library".to_string()),
        1 => Just("Windows".to_string()),
        1 => Just("node_modules".to_string()),
        1 => Just("data.db".to_string()),
    ]
}

fn flip_case(text: &str, mask: &[bool]) -> String {
    text.chars()
        .zip(mask.iter().cycle())
        .map(|(c, flip)| {
            if !*flip {
                c
            } else if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Whatever a rule names is refused, however its wildcards are filled.
    #[test]
    fn everything_a_rule_names_is_refused(platform in any_platform(), pick in any::<prop::sample::Index>(), fill in name()) {
        let all = patterns(platform);
        let (parts, _) = pick.get(&all);
        let path = instantiate(parts, &fill, platform);
        let policy = Policy::new(platform, &[]).unwrap();
        prop_assert!(policy.check_str(&path).is_err(), "{}", path);
    }

    /// Nothing inside a protected tree is allowed unless it is inside one of
    /// the listed holes.
    #[test]
    fn protected_trees_have_no_holes_except_the_listed_ones(
        platform in any_platform(),
        pick in any::<prop::sample::Index>(),
        fill in name(),
        suffix in prop::collection::vec(name(), 1..5),
    ) {
        let trees: Vec<_> = patterns(platform).into_iter().filter(|(_, k)| *k == Kind::Tree).collect();
        let (parts, _) = pick.get(&trees);
        let sep = if platform == Platform::Windows { "\\" } else { "/" };
        let path = format!("{}{sep}{}", instantiate(parts, &fill, platform), suffix.join(sep));
        let policy = Policy::new(platform, &[]).unwrap();
        if policy.check_str(&path).is_ok() {
            prop_assert!(inside_an_open_rule(&split(&path, platform), platform), "hole at {}", path);
        }
    }

    /// A folder that holds anything named by a rule is refused.
    #[test]
    fn every_folder_above_a_rule_is_refused(platform in any_platform(), pick in any::<prop::sample::Index>(), fill in name(), cut in 0usize..8) {
        let all = patterns(platform);
        let (parts, _) = pick.get(&all);
        let first = if platform == Platform::Windows { 1 } else { 0 };
        let keep = first + cut % (parts.len() - first).max(1);
        let ancestor = &parts[..keep.max(first)];
        let path = if ancestor.len() == first && platform != Platform::Windows {
            "/".to_string()
        } else if ancestor.len() == first {
            "C:\\".to_string()
        } else {
            instantiate(ancestor, &fill, platform)
        };
        let policy = Policy::new(platform, &[]).unwrap();
        prop_assert!(policy.check_str(&path).is_err(), "{}", path);
    }

    /// On macOS and Windows, changing letter case changes nothing.
    #[test]
    fn case_never_changes_the_answer(
        mac in any::<bool>(),
        pick in any::<prop::sample::Index>(),
        fill in name(),
        suffix in prop::collection::vec(name(), 0..4),
        mask in prop::collection::vec(any::<bool>(), 1..40),
    ) {
        let platform = if mac { Platform::MacOs } else { Platform::Windows };
        let all = patterns(platform);
        let (parts, _) = pick.get(&all);
        let sep = if mac { "/" } else { "\\" };
        let mut path = instantiate(parts, &fill, platform);
        for s in &suffix {
            path.push_str(sep);
            path.push_str(s);
        }
        let policy = Policy::new(platform, &[]).unwrap();
        let plain = policy.check_str(&path).is_ok();
        let flipped = policy.check_str(&flip_case(&path, &mask)).is_ok();
        prop_assert_eq!(plain, flipped, "{}", path);
    }

    /// On Windows, the separator and the long-path prefix change nothing.
    #[test]
    fn windows_spelling_never_changes_the_answer(
        pick in any::<prop::sample::Index>(),
        fill in name(),
        suffix in prop::collection::vec(name(), 0..4),
    ) {
        let all = patterns(Platform::Windows);
        let (parts, _) = pick.get(&all);
        let mut path = instantiate(parts, &fill, Platform::Windows);
        for s in &suffix {
            path.push('\\');
            path.push_str(s);
        }
        let policy = Policy::new(Platform::Windows, &[]).unwrap();
        let plain = policy.check_str(&path).is_ok();
        prop_assert_eq!(plain, policy.check_str(&path.replace('\\', "/")).is_ok());
        prop_assert_eq!(plain, policy.check_str(&format!(r"\\?\{path}")).is_ok());
        prop_assert_eq!(plain, policy.check_str(&path.replace('\\', r"\\")).is_ok());
    }

    /// No input of any kind crashes the check, and whatever is allowed is a
    /// plain absolute path at least two levels down.
    #[test]
    fn arbitrary_text_never_panics_and_allowed_paths_are_sane(platform in any_platform(), text in "\\PC{0,60}") {
        let policy = Policy::new(platform, &[]).unwrap();
        if policy.check_str(&text).is_ok() {
            let parts = split(&text, platform);
            prop_assert!(!parts.iter().any(|p| p == ".." || p == "."));
            if platform == Platform::Windows {
                prop_assert!(parts.len() >= 2, "{}", text);
            } else {
                prop_assert!(text.starts_with('/') && parts.len() >= 2, "{}", text);
            }
        }
    }

    /// Paths made of path-like pieces: nothing allowed may contain `..`.
    #[test]
    fn dot_dot_is_never_allowed(
        platform in any_platform(),
        before in prop::collection::vec(name(), 0..4),
        after in prop::collection::vec(name(), 0..4),
    ) {
        let sep = if platform == Platform::Windows { "\\" } else { "/" };
        let lead = if platform == Platform::Windows { "D:\\" } else { "/" };
        let mut parts = before;
        parts.push("..".to_string());
        parts.extend(after);
        let path = format!("{lead}{}", parts.join(sep));
        let policy = Policy::new(platform, &[]).unwrap();
        prop_assert!(policy.check_str(&path).is_err(), "{}", path);
    }
}
