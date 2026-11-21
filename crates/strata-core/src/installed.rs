//! Apps the system lists as installed (Windows: the uninstall registry and
//! the Store package repository). Raw facts; `strata-classify` turns them
//! into rules.

/// One installed app as the system lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    /// Stable key: the registry key or the Store package family.
    pub id: String,
    /// Where the program is, as the system recorded it.
    pub program: Option<String>,
    /// Where its data is (Store apps).
    pub data: Option<String>,
}

/// Folder names an install location must never be: shared places that
/// hold many apps' files (a badly written installer can record them).
const SHARED: &[&str] = &[
    "program files",
    "program files (x86)",
    "programdata",
    "common files",
    "windows",
    "system32",
    "syswow64",
    "users",
    "appdata",
    "local",
    "roaming",
    "locallow",
    "programs",
    "desktop",
    "documents",
    "downloads",
    "temp",
    "packages",
    "windowsapps",
];

/// `C:\Program Files\Foo\` -> `C:/Program Files/Foo`, or None when the
/// folder is too broad, not absolute, or holds characters rules reserve.
pub fn usable_folder(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_matches('"').trim_end_matches(['\\', '/']);
    if trimmed.contains('*') || trimmed.contains("..") {
        return None;
    }
    let path = trimmed.replace('\\', "/");
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    // A drive and at least two folders: `C:/Program Files/Foo`.
    let drive = parts.first()?;
    if drive.len() != 2 || !drive.ends_with(':') || parts.len() < 3 {
        return None;
    }
    let last = parts.last()?.to_ascii_lowercase();
    if SHARED.contains(&last.as_str()) || last.len() < 2 {
        return None;
    }
    // Directly inside a user's home is too broad (`C:/Users/sam`).
    if parts.len() == 3 && parts[1].eq_ignore_ascii_case("users") {
        return None;
    }
    Some(parts.join("/"))
}

/// The folder of a program file recorded as an app's icon
/// (`"C:\Program Files\Foo\foo.exe",0`), for apps that record no location.
pub fn folder_of_icon(raw: &str) -> Option<String> {
    let path = raw.split(',').next()?.trim().trim_matches('"');
    if !path.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let cut = path.rfind(['\\', '/'])?;
    usable_folder(&path[..cut])
}

/// Store package full name -> (family name, readable name).
/// `Microsoft.WindowsTerminal_1.21.3231.0_x64__8wekyb3d8bbwe` gives
/// `Microsoft.WindowsTerminal_8wekyb3d8bbwe` and `Windows Terminal`.
pub fn package_family(full: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = full.split('_').collect();
    if parts.len() < 5 {
        return None;
    }
    let (name, publisher) = (parts[0], parts[parts.len() - 1]);
    if name.is_empty() || publisher.is_empty() {
        return None;
    }
    // Frameworks and resource packs belong to no one app.
    let lower = name.to_ascii_lowercase();
    const SHARED_PACKAGES: &[&str] = &[
        "vclibs",
        ".net.",
        "ui.xaml",
        "framework",
        "runtime",
        "services.store",
        "webview",
        "directx",
        "languageexperiencepack",
    ];
    if SHARED_PACKAGES.iter().any(|s| lower.contains(s)) || full.contains("split.") {
        return None;
    }
    let short = name.rsplit('.').next().unwrap_or(name);
    Some((format!("{name}_{publisher}"), spaced(short)))
}

/// `WindowsTerminal` -> `Windows Terminal`, `ZuneMusic` -> `Zune Music`.
fn spaced(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        if i > 0 && c.is_uppercase() && (chars[i - 1].is_lowercase() || next_lower) {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

/// What this computer lists as installed. Empty where not supported yet.
pub fn installed_apps() -> Vec<Installed> {
    imp::installed_apps()
}

#[cfg(not(windows))]
mod imp {
    pub fn installed_apps() -> Vec<super::Installed> {
        Vec::new()
    }
}

#[cfg(windows)]
mod imp {
    use super::{folder_of_icon, package_family, Installed};
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, RRF_RT_REG_DWORD,
        RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const PACKAGES: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle came from RegOpenKeyExW and is closed once.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn open(root: HKEY, path: &str, view: u32) -> Option<Key> {
        let mut key: HKEY = null_mut();
        let name = wide(path);
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let status = unsafe { RegOpenKeyExW(root, name.as_ptr(), 0, KEY_READ | view, &mut key) };
        (status == 0).then_some(Key(key))
    }

    fn subkeys(key: &Key) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = [0u16; 512];
        for i in 0.. {
            let mut len = buf.len() as u32;
            // SAFETY: `buf` holds `len` UTF-16 units; the rest are optional.
            let status = unsafe {
                RegEnumKeyExW(
                    key.0,
                    i,
                    buf.as_mut_ptr(),
                    &mut len,
                    null(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            if status != 0 {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
            if out.len() > 20_000 {
                break; // a damaged registry is no reason to spin
            }
        }
        out
    }

    fn text(key: &Key, sub: &str, value: &str) -> Option<String> {
        let (sub, value) = (wide(sub), wide(value));
        let mut buf = vec![0u16; 2048];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: strings are NUL-terminated; `buf` is `bytes` long.
        let status = unsafe {
            RegGetValueW(
                key.0,
                sub.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != 0 {
            return None;
        }
        let units = (bytes as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..units]);
        let s = s.trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    fn number(key: &Key, sub: &str, value: &str) -> Option<u32> {
        let (sub, value) = (wide(sub), wide(value));
        let mut n: u32 = 0;
        let mut bytes = 4u32;
        // SAFETY: strings are NUL-terminated; `n` is 4 bytes.
        let status = unsafe {
            RegGetValueW(
                key.0,
                sub.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                null_mut(),
                (&mut n as *mut u32).cast(),
                &mut bytes,
            )
        };
        (status == 0).then_some(n)
    }

    pub fn installed_apps() -> Vec<Installed> {
        let mut out = Vec::new();
        let views = [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, "HKLM"),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, "HKLM32"),
            (HKEY_CURRENT_USER, 0, "HKCU"),
        ];
        for (root, view, label) in views {
            let Some(key) = open(root, UNINSTALL, view) else {
                continue;
            };
            for sub in subkeys(&key) {
                // Updates, hotfixes and parts of the system are not apps.
                if number(&key, &sub, "SystemComponent") == Some(1)
                    || text(&key, &sub, "ParentKeyName").is_some()
                    || matches!(
                        text(&key, &sub, "ReleaseType").as_deref(),
                        Some("Update" | "Hotfix" | "Security Update")
                    )
                {
                    continue;
                }
                let Some(name) = text(&key, &sub, "DisplayName") else {
                    continue;
                };
                let program = text(&key, &sub, "InstallLocation")
                    .or_else(|| text(&key, &sub, "DisplayIcon").and_then(|i| folder_of_icon(&i)));
                out.push(Installed {
                    name,
                    id: format!("installed:{label}:{sub}"),
                    program,
                    data: None,
                });
            }
        }
        let local = std::env::var("LOCALAPPDATA").ok();
        if let (Some(key), Some(local)) = (open(HKEY_CURRENT_USER, PACKAGES, 0), local) {
            for full in subkeys(&key) {
                let Some((family, short)) = package_family(&full) else {
                    continue;
                };
                // Store names are often resource references; then the
                // package name, made readable, is the best name there is.
                let name = text(&key, &full, "DisplayName")
                    .filter(|n| !n.starts_with('@') && !n.starts_with("ms-resource:"))
                    .unwrap_or(short);
                out.push(Installed {
                    name,
                    id: format!("store:{family}"),
                    program: text(&key, &full, "PackageRootFolder"),
                    data: Some(format!(r"{local}\Packages\{family}")),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_locations_must_name_one_apps_folder() {
        assert_eq!(
            usable_folder(r"C:\Program Files\Foo\"),
            Some("C:/Program Files/Foo".into())
        );
        assert_eq!(
            usable_folder(r#""D:\Games\Bar Game""#),
            Some("D:/Games/Bar Game".into())
        );
        assert_eq!(
            usable_folder(r"C:\Users\sam\AppData\Local\Programs\Tool"),
            Some("C:/Users/sam/AppData/Local/Programs/Tool".into())
        );
        for broad in [
            r"C:\",
            r"C:\Program Files",
            r"C:\Program Files\Common Files",
            r"C:\Users\sam",
            r"C:\Users\sam\AppData\Local",
            r"C:\Users\sam\AppData\Local\Programs",
            r"C:\Windows\System32",
            r"Program Files\Foo",
            r"C:\Foo\*",
            r"C:\Foo\..\Windows",
            "",
        ] {
            assert_eq!(usable_folder(broad), None, "{broad}");
        }
        assert_eq!(
            folder_of_icon(r#""C:\Program Files\Foo\foo.exe",0"#),
            Some("C:/Program Files/Foo".into())
        );
        assert_eq!(folder_of_icon(r"C:\Program Files\foo.exe"), None);
        assert_eq!(folder_of_icon(r"C:\Program Files\Foo\foo.ico"), None);
    }

    #[test]
    fn store_packages_by_family() {
        assert_eq!(
            package_family("Microsoft.WindowsTerminal_1.21.3231.0_x64__8wekyb3d8bbwe"),
            Some((
                "Microsoft.WindowsTerminal_8wekyb3d8bbwe".into(),
                "Windows Terminal".into()
            ))
        );
        assert_eq!(
            package_family("SpotifyAB.SpotifyMusic_1.250.0.0_x64__zpdnekdrzrea0"),
            Some((
                "SpotifyAB.SpotifyMusic_zpdnekdrzrea0".into(),
                "Spotify Music".into()
            ))
        );
        assert_eq!(
            package_family("Microsoft.VCLibs.140.00_14.0.33519.0_x64__8wekyb3d8bbwe"),
            None
        );
        assert_eq!(
            package_family("Microsoft.UI.Xaml.2.8_8.2310.30001.0_x64__8wekyb3d8bbwe"),
            None
        );
        assert_eq!(package_family("not a package"), None);
    }
}
