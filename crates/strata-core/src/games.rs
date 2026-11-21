//! Games installed by game launchers, one entry per game, from the
//! launchers' own records:
//!
//! - Steam: `steamapps/libraryfolders.vdf` lists every library (on any
//!   drive); each library's `appmanifest_<id>.acf` names a game and its
//!   folder under `steamapps/common`.
//! - Epic: one `.item` manifest per game in
//!   `ProgramData/Epic/EpicGamesLauncher/Data/Manifests`.
//! - Xbox app: a `.GamingRoot` file at the top of a drive names the folder
//!   that holds the games, one folder per game.
//!
//! GOG Galaxy, Battle.net, EA and Ubisoft games register with Windows'
//! uninstall list, so `installed_apps` finds them already.

use crate::installed::Installed;
use std::path::{Path, PathBuf};

/// Values of `"key"  "value"` lines in Valve's text format (`.vdf`,
/// `.acf`), in order. Escapes (`\\`, `\"`) are undone.
pub fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('"') else {
            continue;
        };
        let Some((k, rest)) = rest.split_once('"') else {
            continue;
        };
        if !k.eq_ignore_ascii_case(key) {
            continue;
        }
        let rest = rest.trim_start();
        if let Some(v) = quoted(rest) {
            out.push(v);
        }
    }
    out
}

/// The first quoted string at the start of `s`, unescaped.
fn quoted(s: &str) -> Option<String> {
    let mut chars = s.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            '"' => return Some(out),
            c => out.push(c),
        }
    }
    None
}

/// A string field of a JSON object, found by name (Epic manifests are
/// flat JSON; a full parser is not needed to read three fields).
pub fn json_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = text.find(&needle)? + needle.len();
    let rest = text[at..].trim_start().strip_prefix(':')?.trim_start();
    quoted(rest)
}

/// The games folder named by a drive's `.GamingRoot` file: the bytes
/// `RGBX`, a count, then UTF-16 folder names ending in a zero.
pub fn gaming_root(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 10 || &bytes[..4] != b"RGBX" {
        return None;
    }
    let units: Vec<u16> = bytes[8..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|u| *u != 0)
        .collect();
    let name = String::from_utf16(&units).ok()?;
    let name = name.trim().trim_start_matches(['\\', '/']).to_string();
    (!name.is_empty()).then_some(name)
}

fn read(path: &Path) -> Option<String> {
    // Launcher records are small; anything large is not one.
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Steam games in every library listed by the Steam install at `steam`.
pub fn steam_games(steam: &Path) -> Vec<Installed> {
    let mut libraries: Vec<PathBuf> = vec![steam.to_path_buf()];
    if let Some(text) = read(&steam.join("steamapps").join("libraryfolders.vdf")) {
        for p in vdf_values(&text, "path") {
            let p = PathBuf::from(p);
            if !libraries.contains(&p) {
                libraries.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for lib in libraries {
        let apps = lib.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            continue;
        };
        for e in entries.flatten().take(5000) {
            let file = e.file_name().to_string_lossy().into_owned();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Some(text) = read(&e.path()) else {
                continue;
            };
            let (Some(id), Some(name), Some(dir)) = (
                vdf_values(&text, "appid").into_iter().next(),
                vdf_values(&text, "name").into_iter().next(),
                vdf_values(&text, "installdir").into_iter().next(),
            ) else {
                continue;
            };
            if dir.contains(['/', '\\']) || dir == ".." || dir.is_empty() {
                continue;
            }
            out.push(Installed {
                name,
                id: format!("steam:{id}"),
                program: Some(apps.join("common").join(dir).to_string_lossy().into_owned()),
                data: None,
                game: true,
            });
        }
    }
    out
}

/// Epic games from the launcher's manifests folder.
pub fn epic_games(manifests: &Path) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(manifests) else {
        return Vec::new();
    };
    entries
        .flatten()
        .take(5000)
        .filter(|e| e.path().extension().is_some_and(|x| x == "item"))
        .filter_map(|e| {
            let text = read(&e.path())?;
            Some(Installed {
                name: json_field(&text, "DisplayName")?,
                id: format!("epic:{}", json_field(&text, "AppName")?),
                program: Some(json_field(&text, "InstallLocation")?),
                data: None,
                game: true,
            })
        })
        .collect()
}

/// Xbox app games on the drive whose top folder is `drive`.
pub fn xbox_games(drive: &Path) -> Vec<Installed> {
    let Some(folder) = std::fs::read(drive.join(".GamingRoot"))
        .ok()
        .and_then(|b| gaming_root(&b))
    else {
        return Vec::new();
    };
    if folder.contains("..") {
        return Vec::new();
    }
    let root = drive.join(&folder);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .take(2000)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            Installed {
                id: format!("xbox:{name}"),
                name,
                program: Some(e.path().to_string_lossy().into_owned()),
                data: None,
                game: true,
            }
        })
        .collect()
}

/// Every game the launchers on this computer record.
pub fn installed_games() -> Vec<Installed> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let program_files_x86 =
        std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
    out.extend(steam_games(&Path::new(&program_files_x86).join("Steam")));
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
    out.extend(epic_games(
        &Path::new(&program_data).join(r"Epic\EpicGamesLauncher\Data\Manifests"),
    ));
    for letter in b'C'..=b'Z' {
        let drive = PathBuf::from(format!("{}:\\", letter as char));
        if drive.join(".GamingRoot").is_file() {
            out.extend(xbox_games(&drive));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valve_text_format() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { "730" "35000000000" }
	}
}"#;
        assert_eq!(
            vdf_values(vdf, "path"),
            [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]
        );
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"installdir\"\t\t\"Counter-Strike Global Offensive\"\n}";
        assert_eq!(vdf_values(acf, "name"), ["Counter-Strike 2"]);
        assert_eq!(
            vdf_values(acf, "installdir"),
            ["Counter-Strike Global Offensive"]
        );
    }

    #[test]
    fn epic_manifest_fields() {
        let item = r#"{ "FormatVersion": 0, "DisplayName": "Fortnite", "AppName": "Fortnite",
            "InstallLocation": "C:\\Program Files\\Epic Games\\Fortnite", "InstallSize": 1 }"#;
        assert_eq!(json_field(item, "DisplayName").as_deref(), Some("Fortnite"));
        assert_eq!(
            json_field(item, "InstallLocation").as_deref(),
            Some(r"C:\Program Files\Epic Games\Fortnite")
        );
        assert_eq!(json_field(item, "Missing"), None);
    }

    #[test]
    fn xbox_gaming_root() {
        let mut bytes = b"RGBX\x01\x00\x00\x00".to_vec();
        for u in "\\XboxGames".encode_utf16().chain([0]) {
            bytes.extend(u.to_le_bytes());
        }
        assert_eq!(gaming_root(&bytes).as_deref(), Some("XboxGames"));
        assert_eq!(gaming_root(b"nope"), None);
    }

    #[test]
    fn steam_libraries_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let other = tmp.path().join("Library2");
        std::fs::create_dir_all(steam.join("steamapps")).unwrap();
        std::fs::create_dir_all(other.join("steamapps/common/Hades")).unwrap();
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\"1\"\n{{\n\"path\"\t\"{}\"\n}}\n}}",
            other.to_string_lossy().replace('\\', "\\\\")
        );
        std::fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        std::fs::write(
            other.join("steamapps/appmanifest_1145360.acf"),
            "\"AppState\"\n{\n\"appid\" \"1145360\"\n\"name\" \"Hades\"\n\"installdir\" \"Hades\"\n}",
        )
        .unwrap();
        // A manifest pointing outside its library is ignored.
        std::fs::write(
            other.join("steamapps/appmanifest_1.acf"),
            "\"appid\" \"1\"\n\"name\" \"Bad\"\n\"installdir\" \"..\\\\..\\\\Windows\"\n",
        )
        .unwrap();
        let games = steam_games(&steam);
        assert_eq!(games.len(), 1, "{games:?}");
        assert_eq!(games[0].name, "Hades");
        assert_eq!(games[0].id, "steam:1145360");
        assert!(games[0].game);
        assert!(games[0].program.as_deref().unwrap().ends_with("Hades"));
    }
}
