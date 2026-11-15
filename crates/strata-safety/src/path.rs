//! Paths as the policy sees them: a root and a list of folded components.
//!
//! Parsing is strict. Anything that is not a plain absolute path is refused
//! instead of being tidied up, because every tidy-up rule is a way for a
//! path to mean something other than what it appears to say.

use crate::{Platform, Reason};

/// A parsed absolute path. Components are folded for comparison (see [`fold`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Parsed {
    /// `'/'` on Unix systems, the lower-case drive letter on Windows.
    pub root: char,
    pub parts: Vec<String>,
}

impl Parsed {
    /// True when `self` is `other` or lies below it.
    pub fn starts_with(&self, other: &Parsed) -> bool {
        self.root == other.root
            && self.parts.len() >= other.parts.len()
            && self.parts[..other.parts.len()] == other.parts[..]
    }
}

/// Make names that the file system treats as the same compare equal.
///
/// macOS and Windows volumes ignore case, so `/SYSTEM` is `/System`. Their
/// case rules also equate a few non-ASCII characters with ASCII letters
/// (`ſ` with `s`, the Kelvin sign with `k`, `ß` and ligatures with letter
/// pairs). Folding more than a given file system does only ever causes an
/// extra refusal, never a missed one, so every such mapping is applied.
pub(crate) fn fold(component: &str, platform: Platform) -> String {
    if platform == Platform::Linux {
        return component.to_string();
    }
    let mut out = String::with_capacity(component.len());
    for c in component.chars() {
        match c {
            'ſ' => out.push('s'),
            '\u{212A}' => out.push('k'),
            'ß' | 'ẞ' => out.push_str("ss"),
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

pub(crate) fn parse(text: &str, platform: Platform) -> Result<Parsed, Reason> {
    if text.is_empty() {
        return Err(Reason::NotAbsolute);
    }
    if text.chars().any(|c| c == '\0') {
        return Err(Reason::UnusualPath);
    }
    match platform {
        Platform::Windows => parse_windows(text),
        Platform::MacOs | Platform::Linux => parse_unix(text, platform),
    }
}

fn parse_unix(text: &str, platform: Platform) -> Result<Parsed, Reason> {
    let Some(rest) = text.strip_prefix('/') else {
        return Err(Reason::NotAbsolute);
    };
    let mut parts = Vec::new();
    for part in rest.split('/') {
        match part {
            "" => {} // doubled or trailing slash
            "." | ".." => return Err(Reason::UnusualPath),
            _ => parts.push(fold(part, platform)),
        }
    }
    Ok(Parsed { root: '/', parts })
}

fn parse_windows(text: &str) -> Result<Parsed, Reason> {
    // `\\?\C:\…` is the long-path spelling of `C:\…`.
    let text = match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case(r"UNC\") => {
            return Err(Reason::NetworkPath)
        }
        Some(rest) => rest,
        None => text,
    };
    let is_sep = |c: char| c == '\\' || c == '/';
    if text.starts_with(is_sep) {
        // `\\server\share`, `\\.\device`, or a path relative to the current drive.
        return Err(if text[1..].starts_with(is_sep) {
            Reason::NetworkPath
        } else {
            Reason::NotAbsolute
        });
    }
    let mut chars = text.chars();
    let (Some(drive), Some(':')) = (chars.next(), chars.next()) else {
        return Err(Reason::NotAbsolute);
    };
    if !drive.is_ascii_alphabetic() {
        return Err(Reason::NotAbsolute);
    }
    let rest = chars.as_str();
    if !rest.starts_with(is_sep) {
        return Err(Reason::NotAbsolute); // `C:` or `C:folder` depend on hidden state
    }

    let mut parts = Vec::new();
    for part in rest.split(is_sep) {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." {
            return Err(Reason::UnusualPath);
        }
        // Windows drops trailing dots and spaces, so `Windows.` is `Windows`.
        if part.ends_with('.') || part.ends_with(' ') {
            return Err(Reason::UnusualPath);
        }
        // A colon selects an alternate data stream; the rest are never legal in names.
        if part
            .chars()
            .any(|c| matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*') || c.is_control())
        {
            return Err(Reason::UnusualPath);
        }
        // `PROGRA~1` is another name for `Program Files`. The real name cannot
        // be known from the text alone, so short-name lookalikes are refused.
        if looks_like_short_name(part) {
            return Err(Reason::UnusualPath);
        }
        // `NUL`, `CON`, `COM1` and the like name devices, not files, in any
        // folder and with any extension.
        if is_device_name(part) {
            return Err(Reason::UnusualPath);
        }
        parts.push(fold(part, Platform::Windows));
    }
    Ok(Parsed {
        root: drive.to_ascii_lowercase(),
        parts,
    })
}

fn looks_like_short_name(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0] == b'~' && w[1].is_ascii_digit())
}

fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (upper.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.as_bytes()[3].is_ascii_digit())
}
