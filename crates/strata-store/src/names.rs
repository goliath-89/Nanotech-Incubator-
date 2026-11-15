//! File names are stored exactly as the operating system gave them, not as
//! text: a name that is not valid Unicode must still lead back to its file.

use std::ffi::{OsStr, OsString};

#[cfg(unix)]
pub(crate) fn encode(name: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().to_vec()
}

#[cfg(unix)]
pub(crate) fn decode(bytes: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes)
}

/// Windows names are 16-bit units; store them little-endian.
#[cfg(windows)]
pub(crate) fn encode(name: &OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    name.encode_wide().flat_map(u16::to_le_bytes).collect()
}

#[cfg(windows)]
pub(crate) fn decode(bytes: Vec<u8>) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    OsString::from_wide(&wide)
}
