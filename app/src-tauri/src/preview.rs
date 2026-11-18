//! Preview a file from a scan without opening it in another program.
//!
//! macOS: Quick Look (`qlmanage -p`), the system's own previewer.
//! Elsewhere: images go to the window as data (the web view decodes them,
//! sandboxed), text files as their first 64 KB of text. Nothing is ever run
//! or opened with another program, links are not followed, and cloud-only
//! files are never read (reading would make OneDrive or iCloud download
//! them).

use crate::bridge::{path_in_scan, AppState};
use serde::Serialize;
#[cfg(not(target_os = "macos"))]
use std::io::Read;
use std::path::Path;
use tauri::State;

#[cfg_attr(target_os = "macos", allow(dead_code))]
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
#[cfg_attr(target_os = "macos", allow(dead_code))]
const TEXT_BYTES: usize = 64 * 1024;

#[cfg_attr(target_os = "macos", allow(dead_code))]
const IMAGES: &[(&str, &str)] = &[
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
    ("ico", "image/x-icon"),
    ("avif", "image/avif"),
];

#[cfg_attr(target_os = "macos", allow(dead_code))]
const TEXT: &[&str] = &[
    "txt", "md", "log", "csv", "tsv", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf",
    "rs", "ts", "js", "py", "java", "c", "h", "cpp", "cs", "go", "rb", "php", "sh", "ps1", "bat",
    "sql", "html", "css", "svg",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Preview {
    /// Shown by the system (Quick Look on macOS).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Native,
    Image {
        mime: &'static str,
        data: String,
    },
    Text {
        text: String,
        truncated: bool,
    },
    /// Only in the cloud: not read, so it is not downloaded.
    Cloud,
    /// A kind of file with no preview here.
    None,
}

#[tauri::command]
pub fn preview_file(
    state: State<'_, AppState>,
    scan_id: i64,
    dir_id: u32,
    file: String,
) -> Result<Preview, String> {
    let path = path_in_scan(&state, scan_id, dir_id, Some(file))?;
    preview_at(&path)
}

/// The checks every preview goes through, then the preview itself.
pub(crate) fn preview_at(path: &Path) -> Result<Preview, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| format!("{} is no longer there", path.display()))?;
    if !meta.file_type().is_file() {
        return Ok(Preview::None); // links, folders, devices: never followed
    }
    if cloud_only(&meta) {
        return Ok(Preview::Cloud);
    }
    preview_path(path, meta.len())
}

#[cfg(target_os = "macos")]
fn preview_path(path: &Path, _len: u64) -> Result<Preview, String> {
    // Arguments go straight to the program; no shell is involved.
    std::process::Command::new("qlmanage")
        .arg("-p")
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| Preview::Native)
        .map_err(|e| format!("could not open Quick Look: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn preview_path(path: &Path, len: u64) -> Result<Preview, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if let Some(&(_, mime)) = IMAGES.iter().find(|(e, _)| *e == ext) {
        if len > MAX_IMAGE_BYTES {
            return Ok(Preview::None);
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        return Ok(Preview::Image {
            mime,
            data: base64(&bytes),
        });
    }
    if TEXT.contains(&ext.as_str()) {
        let mut buf = Vec::with_capacity(TEXT_BYTES.min(len as usize));
        std::fs::File::open(path)
            .and_then(|f| f.take(TEXT_BYTES as u64).read_to_end(&mut buf))
            .map_err(|e| e.to_string())?;
        return Ok(Preview::Text {
            text: String::from_utf8_lossy(&buf).into_owned(),
            truncated: len > TEXT_BYTES as u64,
        });
    }
    Ok(Preview::None)
}

#[cfg(windows)]
fn cloud_only(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x40000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    meta.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(target_os = "macos")]
fn cloud_only(meta: &std::fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const SF_DATALESS: u32 = 0x4000_0000;
    meta.st_flags() & SF_DATALESS != 0
}

#[cfg(not(any(windows, target_os = "macos")))]
fn cloud_only(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
/// Standard base64 (RFC 4648) for a data: URL.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn previews_images_and_text_and_never_follows_links() {
        let dir = std::env::temp_dir().join(format!("strata-preview-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let png = dir.join("a.PNG");
        std::fs::write(&png, b"\x89PNG fake").unwrap();
        match preview_at(&png).unwrap() {
            Preview::Image { mime, data } => {
                assert_eq!(mime, "image/png");
                assert_eq!(data, base64(b"\x89PNG fake"));
            }
            _ => panic!("expected an image"),
        }
        let log = dir.join("big.log");
        std::fs::write(&log, "x".repeat(TEXT_BYTES + 10)).unwrap();
        match preview_at(&log).unwrap() {
            Preview::Text { text, truncated } => {
                assert_eq!(text.len(), TEXT_BYTES);
                assert!(truncated);
            }
            _ => panic!("expected text"),
        }
        // A link to a text file is not followed.
        let link = dir.join("link.txt");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&log, &link).unwrap();
        assert!(matches!(preview_at(&link).unwrap(), Preview::None));
        // Programs and unknown kinds get no preview.
        let exe = dir.join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        assert!(matches!(preview_at(&exe).unwrap(), Preview::None));
        assert!(matches!(preview_at(&dir).unwrap(), Preview::None));
        assert!(preview_at(&dir.join("missing.txt")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_matches_the_standard_test_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }
}
