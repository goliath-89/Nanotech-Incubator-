//! What Strata may move to the Trash.
//!
//! This crate is the only code in Strata that removes anything, and it can
//! only move things to the Trash or Recycle Bin. Every request passes through
//! [`Policy::check`], which refuses operating-system folders, programs, the
//! folders that hold a user's files, and anything that contains one of those.
//!
//! The policy is plain text matching with no file access, so the rules for
//! macOS and Windows can be (and are) tested on any machine.

#![forbid(unsafe_code)]

mod access;
mod path;
mod rules;
mod trash;

pub use access::can_remove;
pub use trash::{trash, vet, TrashReport, VetError};

use path::{fold, parse, Parsed};
use rules::Kind;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    /// The platform this build runs on.
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// Why something may not be moved to the Trash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Not a full path from the top of a drive.
    NotAbsolute,
    /// Contains `..`, a stream name, a short-name alias or another form whose
    /// real target cannot be told from the text.
    UnusualPath,
    /// A network location.
    NetworkPath,
    /// A whole drive.
    VolumeRoot,
    /// A folder directly at the top of the system drive.
    TopLevel,
    /// Named on the protected list, or inside a protected folder.
    Protected,
    /// A folder that holds something on the protected list.
    ContainsProtected,
    /// A program, or part of one.
    InsideApplication,
    /// Part of a library or document that is stored as a folder.
    InsidePackage,
    /// Not inside the folder that was scanned.
    OutsideScan,
    /// A symbolic link or junction.
    Link,
    /// Another drive mounted at this folder.
    MountPoint,
}

/// A refusal, with the rule that caused it when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: Reason,
    /// The protected path that matched, as written in the policy.
    pub rule: Option<String>,
}

impl Refusal {
    fn new(reason: Reason) -> Refusal {
        Refusal { reason, rule: None }
    }
}

impl From<Reason> for Refusal {
    fn from(reason: Reason) -> Refusal {
        Refusal::new(reason)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self.reason {
            Reason::NotAbsolute => "it is not a full path",
            Reason::UnusualPath => "its path is written in a form Strata does not act on",
            Reason::NetworkPath => "it is on a network location",
            Reason::VolumeRoot => "it is a whole drive",
            Reason::TopLevel => "it sits at the top of the system drive",
            Reason::Protected => "it is needed by the system or holds your files",
            Reason::ContainsProtected => "it contains something the system or your apps need",
            Reason::InsideApplication => "it is an app or part of one",
            Reason::InsidePackage => "it is part of a library or document",
            Reason::OutsideScan => "it is outside the folder that was scanned",
            Reason::Link => "it is a link to something else",
            Reason::MountPoint => "another drive is attached at this folder",
        };
        match &self.rule {
            Some(rule) => write!(f, "{text} ({rule})"),
            None => f.write_str(text),
        }
    }
}

#[derive(Clone, Debug)]
struct Rule {
    /// `'/'`, a lower-case drive letter, or `'*'` for every drive.
    root: char,
    /// Folded names; one ending in `*` matches by prefix.
    parts: Vec<String>,
    kind: Kind,
    /// As written, for messages.
    label: String,
}

enum Relation {
    Unrelated,
    /// The path contains the rule's target.
    Contains,
    Equal,
    Inside,
}

impl Rule {
    fn relation(&self, path: &Parsed) -> Relation {
        if self.root != '*' && self.root != path.root {
            return Relation::Unrelated;
        }
        let shared = self.parts.len().min(path.parts.len());
        for i in 0..shared {
            let (pattern, name) = (&self.parts[i], &path.parts[i]);
            let matches = match pattern.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => pattern == name,
            };
            if !matches {
                return Relation::Unrelated;
            }
        }
        match path.parts.len().cmp(&self.parts.len()) {
            std::cmp::Ordering::Less => Relation::Contains,
            std::cmp::Ordering::Equal => Relation::Equal,
            std::cmp::Ordering::Greater => Relation::Inside,
        }
    }
}

/// The set of rules for one platform and one machine.
#[derive(Clone, Debug)]
pub struct Policy {
    platform: Platform,
    rules: Vec<Rule>,
}

impl Policy {
    /// The built-in rules for `platform`. `extra_homes` are home folders that
    /// are not in the platform's usual place (`/Users/name`, `C:\Users\name`,
    /// `/home/name`); each must be a full path.
    pub fn new(platform: Platform, extra_homes: &[&str]) -> Result<Policy, Reason> {
        let (table, homes) = match platform {
            Platform::MacOs => (rules::MACOS, rules::MACOS_HOMES),
            Platform::Windows => (rules::WINDOWS, rules::WINDOWS_HOMES),
            Platform::Linux => (rules::LINUX, rules::LINUX_HOMES),
        };
        let mut home_patterns: Vec<(char, Vec<String>, String)> = homes
            .iter()
            .map(|h| {
                let (root, parts) = parse_pattern(h, platform);
                (root, parts, h.to_string())
            })
            .collect();
        for home in extra_homes {
            let parsed = parse(home, platform)?;
            if parsed.parts.is_empty() {
                return Err(Reason::VolumeRoot);
            }
            home_patterns.push((parsed.root, parsed.parts, home.to_string()));
        }

        let mut policy = Policy {
            platform,
            rules: Vec::new(),
        };
        for (pattern, kind) in table {
            match pattern.strip_prefix('~') {
                Some(rest) => {
                    for (root, home_parts, home_label) in &home_patterns {
                        let (_, tail) = parse_pattern(rest, platform);
                        let mut parts = home_parts.clone();
                        parts.extend(tail);
                        policy.rules.push(Rule {
                            root: *root,
                            parts,
                            kind: *kind,
                            label: format!("{home_label}{rest}"),
                        });
                    }
                }
                None => {
                    let (root, parts) = parse_pattern(pattern, platform);
                    policy.rules.push(Rule {
                        root,
                        parts,
                        kind: *kind,
                        label: pattern.to_string(),
                    });
                }
            }
        }
        Ok(policy)
    }

    /// The rules for this machine and the person using it: the built-in
    /// list, their home folder wherever it is, and Strata's own files.
    pub fn for_current_user() -> Result<Policy, Reason> {
        let platform = Platform::current();
        let home = std::env::var_os(if platform == Platform::Windows {
            "USERPROFILE"
        } else {
            "HOME"
        });
        let home = home.map(|h| h.to_string_lossy().into_owned());
        let extra: Vec<&str> = home.iter().map(String::as_str).collect();
        let mut policy = Policy::new(platform, &extra)?;
        if let Some(folder) = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(Path::parent)
        {
            // Strata must never remove itself. A build run from an odd place
            // (a volume root, say) is covered by the other rules.
            let _ = policy.protect(&folder.to_string_lossy());
        }
        Ok(policy)
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Protect one more folder and everything in it (Strata's own data, say).
    /// There is deliberately no way to do the opposite.
    pub fn protect(&mut self, folder: &str) -> Result<(), Reason> {
        let parsed = parse(folder, self.platform)?;
        if parsed.parts.is_empty() {
            return Err(Reason::VolumeRoot);
        }
        self.rules.push(Rule {
            root: parsed.root,
            parts: parsed.parts,
            kind: Kind::Tree,
            label: folder.to_string(),
        });
        Ok(())
    }

    /// May `path` be moved to the Trash? Looks only at the text of the path.
    pub fn check(&self, path: &Path) -> Result<(), Refusal> {
        // A name that is not valid text becomes replacement characters here.
        // Such a name can then only fail to match an allowance, never gain one.
        self.check_str(&path.to_string_lossy())
    }

    pub fn check_str(&self, path: &str) -> Result<(), Refusal> {
        let parsed = parse(path, self.platform)?;
        self.check_parsed(&parsed)
    }

    pub(crate) fn parse(&self, path: &Path) -> Result<Parsed, Reason> {
        parse(&path.to_string_lossy(), self.platform)
    }

    fn check_parsed(&self, path: &Parsed) -> Result<(), Refusal> {
        if path.parts.is_empty() {
            return Err(Reason::VolumeRoot.into());
        }

        // The longest rule that names the path or a folder above it decides.
        let mut deciding: Option<(&Rule, bool)> = None;
        for rule in &self.rules {
            let equal = match rule.relation(path) {
                Relation::Unrelated => continue,
                Relation::Contains => {
                    return Err(Refusal {
                        reason: Reason::ContainsProtected,
                        rule: Some(rule.label.clone()),
                    })
                }
                Relation::Equal => true,
                Relation::Inside => false,
            };
            let better = match deciding {
                None => true,
                Some((best, _)) => (rule.parts.len(), rule.kind) > (best.parts.len(), best.kind),
            };
            if better {
                deciding = Some((rule, equal));
            }
        }
        let protected = |rule: &Rule| Refusal {
            reason: Reason::Protected,
            rule: Some(rule.label.clone()),
        };
        match deciding {
            Some((rule, _)) if rule.kind == Kind::Tree => return Err(protected(rule)),
            // Exact protects the folder itself; Open keeps its folder too.
            Some((rule, true)) => return Err(protected(rule)),
            Some((_, false)) | None => {}
        }

        if self.platform == Platform::MacOs {
            let last = path.parts.len() - 1;
            for (i, part) in path.parts.iter().enumerate() {
                if rules::MACOS_PROGRAM_SUFFIXES
                    .iter()
                    .any(|s| part.ends_with(s))
                {
                    return Err(Reason::InsideApplication.into());
                }
                if i < last
                    && rules::MACOS_PACKAGE_SUFFIXES
                        .iter()
                        .any(|s| part.ends_with(s))
                {
                    return Err(Reason::InsidePackage.into());
                }
            }
        }

        // Whatever sits directly at the top of a Unix system drive is either
        // the system's or was put there on purpose by an administrator.
        if path.root == '/' && path.parts.len() == 1 {
            return Err(Reason::TopLevel.into());
        }
        Ok(())
    }
}

/// Parse a rule pattern from `rules.rs`. Patterns are trusted constants.
fn parse_pattern(pattern: &str, platform: Platform) -> (char, Vec<String>) {
    let mut pieces = pattern.split('/');
    let first = pieces.next().unwrap_or("");
    let root = match first.strip_suffix(':') {
        Some(drive) => drive
            .chars()
            .next()
            .map(|c| c.to_ascii_lowercase())
            .unwrap_or('*'),
        None => '/',
    };
    let parts = pieces
        .filter(|p| !p.is_empty())
        .map(|p| fold(p, platform))
        .collect();
    (root, parts)
}

#[cfg(test)]
mod tests;
