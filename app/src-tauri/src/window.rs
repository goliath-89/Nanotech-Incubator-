//! The main window, and the system material behind its sidebar.
//!
//! On Windows 11 the window is created transparent and given the Mica
//! material; the page then makes only its sidebar see-through. Everywhere
//! else (Windows 10, Linux, and macOS until vibrancy is decided on Day 16)
//! the window is an ordinary opaque one.

use tauri::{App, State, WebviewUrl, WebviewWindowBuilder};

/// Which material the window got: "mica" or "none".
pub struct Material(pub &'static str);

pub fn create(app: &App) -> tauri::Result<Material> {
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Strata")
        .inner_size(1180.0, 760.0)
        .min_inner_size(820.0, 560.0);
    #[cfg(windows)]
    {
        use tauri::window::{Effect, EffectsBuilder};
        let mica = windows_11();
        let window = builder.transparent(mica).build()?;
        if mica {
            window.set_effects(EffectsBuilder::new().effect(Effect::Mica).build())?;
            return Ok(Material("mica"));
        }
        Ok(Material("none"))
    }
    #[cfg(not(windows))]
    {
        builder.build()?;
        Ok(Material("none"))
    }
}

/// The page asks once, at start, so it can make the sidebar see-through.
#[tauri::command]
pub fn window_material(material: State<'_, Material>) -> &'static str {
    material.0
}

/// Mica needs Windows 11 (build 22000 or later).
#[cfg(windows)]
fn windows_11() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let value = wide("CurrentBuildNumber");
    let mut buffer = [0u16; 32];
    let mut len = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: the key and value names are NUL-terminated and outlive the
    // call; `buffer` is writable for `len` bytes, which the call updates.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if status != 0 {
        return false;
    }
    let chars = (len as usize / 2).min(buffer.len());
    String::from_utf16_lossy(&buffer[..chars])
        .trim_end_matches('\0')
        .trim()
        .parse::<u32>()
        .is_ok_and(|build| build >= 22000)
}
