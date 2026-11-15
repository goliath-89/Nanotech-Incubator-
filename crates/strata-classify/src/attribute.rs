//! Work out which app each folder of a scan belongs to.
//!
//! Runs after the scan, on folders only. A folder belongs to the app whose
//! rule names it or the nearest folder above it; a deeper rule wins over a
//! shallower one (an app's cache folder inside its data folder is cache).
//! Folders no rule reaches belong to no app. Nothing is guessed.

use crate::rules::{pattern_parts, Os, Role, RuleSet};
use std::path::Path;
use strata_core::{Kind, ScanTree};

/// Where things are on the machine that was scanned.
#[derive(Clone, Debug)]
pub struct Env {
    pub os: Os,
    /// The user's home folder, as an absolute path with `/` or `\`.
    pub home: String,
    /// On Windows, the drive Windows is installed on, e.g. `"C:"`.
    pub system_drive: String,
}

impl Env {
    /// The machine this program is running on.
    pub fn current() -> Option<Env> {
        let os = Os::current();
        let home = std::env::var(if os == Os::Windows { "USERPROFILE" } else { "HOME" }).ok()?;
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        Some(Env { os, home, system_drive })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner {
    /// Index into [`Attribution::apps`].
    pub app: u32,
    pub role: Role,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppUsage {
    pub name: String,
    pub id: String,
    pub kind: Option<Kind>,
    /// Bytes of the program itself.
    pub app_bytes: u64,
    /// Bytes of what it created.
    pub data_bytes: u64,
    /// Bytes it would rebuild.
    pub cache_bytes: u64,
}

impl AppUsage {
    pub fn total_bytes(&self) -> u64 {
        self.app_bytes + self.data_bytes + self.cache_bytes
    }
}

#[derive(Clone, Debug)]
pub struct Attribution {
    /// Apps that own at least one byte, largest first.
    pub apps: Vec<AppUsage>,
    /// For each folder of the scan (same indexes as `ScanTree::dirs`).
    pub owners: Vec<Option<Owner>>,
    /// Folders a `cache` rule named directly: candidates for cleanup.
    pub cache_dirs: Vec<u32>,
}

fn split_path(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|p| !p.is_empty()).collect()
}

struct Matcher {
    ignore_case: bool,
}

impl Matcher {
    /// `pattern` is a literal name, `Start*`, `*end` or `*`.
    fn eq(&self, pattern: &str, name: &str) -> bool {
        let fold = |s: &str| if self.ignore_case { s.to_lowercase() } else { s.to_string() };
        let (pattern, name) = (fold(pattern), fold(name));
        if let Some(prefix) = pattern.strip_suffix('*') {
            name.starts_with(prefix)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else {
            pattern == name
        }
    }
}

pub fn attribute(tree: &ScanTree, rules: &RuleSet, env: &Env) -> Attribution {
    let matcher = Matcher {
        ignore_case: env.os != Os::Linux,
    };
    let root_text = tree.root().name.to_string_lossy().into_owned();
    let root_parts = split_path(&root_text);
    let home_parts = split_path(&env.home);

    // Names once, as text, for matching.
    let names: Vec<String> = tree.dirs.iter().map(|d| d.name.to_string_lossy().into_owned()).collect();

    // (folder, owner, how many path parts the rule had)
    let mut direct: Vec<Option<(Owner, usize)>> = vec![None; tree.dirs.len()];
    let mut cache_dirs: Vec<u32> = Vec::new();
    let mut usages: Vec<AppUsage> = rules
        .apps
        .iter()
        .map(|a| AppUsage {
            name: a.name.clone(),
            id: a.id.clone(),
            kind: a.kind,
            app_bytes: 0,
            data_bytes: 0,
            cache_bytes: 0,
        })
        .collect();

    for (app_index, app) in rules.apps.iter().enumerate() {
        for (pattern, role) in &app.paths {
            // Turn the pattern into absolute path parts on this machine.
            let mut parts: Vec<&str> = Vec::new();
            let written = pattern_parts(pattern);
            let mut rest = &written[..];
            if written.first() == Some(&"~") {
                parts.extend(&home_parts);
                rest = &written[1..];
            } else if written.first().is_some_and(|p| p.ends_with(':')) {
                parts.push(&env.system_drive);
                rest = &written[1..];
            }
            parts.extend(rest);

            let owner = Owner {
                app: app_index as u32,
                role: *role,
            };
            let mut hits = Vec::new();
            if parts.len() >= root_parts.len() {
                // The rule is at or below the scanned folder: walk down to it.
                let (above, below) = parts.split_at(root_parts.len());
                if above.iter().zip(&root_parts).all(|(p, r)| matcher.eq(p, r)) {
                    descend(tree, &names, &matcher, 0, below, &mut hits);
                }
            } else if parts.iter().zip(&root_parts).all(|(p, r)| matcher.eq(p, r)) {
                // The scanned folder is inside the rule's folder: all of it belongs.
                hits.push(0);
            }
            for dir in hits {
                let better = match direct[dir as usize] {
                    None => true,
                    Some((_, depth)) => parts.len() > depth,
                };
                if better {
                    direct[dir as usize] = Some((owner, parts.len()));
                }
                if *role == Role::Cache {
                    cache_dirs.push(dir);
                }
            }
        }
    }

    // Programs no rule names still are programs: each folder directly inside
    // an applications folder is an app called by its own name.
    for (i, dir) in tree.dirs.iter().enumerate() {
        if direct[i].is_some() {
            continue;
        }
        let Some(parent) = dir.parent else { continue };
        let full = tree.dir_path(parent);
        if !is_programs_folder(&full, env, &matcher) {
            continue;
        }
        let name = &names[i];
        let display = match env.os {
            Os::MacOs => match name.len().checked_sub(4) {
                Some(cut) if name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(".app") => &name[..cut],
                _ => continue, // loose folders in /Applications are not apps
            },
            _ => name.as_str(),
        };
        let app = usages.len() as u32;
        usages.push(AppUsage {
            name: display.to_string(),
            id: format!("folder:{name}"),
            kind: None,
            app_bytes: 0,
            data_bytes: 0,
            cache_bytes: 0,
        });
        direct[i] = Some((Owner { app, role: Role::App }, usize::MAX));
    }

    // Hand owners down the tree (parents come before children) and add up
    // each folder's own bytes: its total minus its child folders' totals.
    let mut owners: Vec<Option<Owner>> = Vec::with_capacity(tree.dirs.len());
    for (i, dir) in tree.dirs.iter().enumerate() {
        let owner = match direct[i] {
            Some((owner, _)) => Some(owner),
            None => dir.parent.and_then(|p| owners[p as usize]),
        };
        owners.push(owner);
        if let Some(owner) = owner {
            let below: u64 = dir.children.iter().map(|&c| tree.dirs[c as usize].alloc_bytes).sum();
            let own = dir.alloc_bytes - below;
            let usage = &mut usages[owner.app as usize];
            match owner.role {
                Role::App => usage.app_bytes += own,
                Role::Data => usage.data_bytes += own,
                Role::Cache => usage.cache_bytes += own,
            }
        }
    }

    // Report apps largest first; re-point the owners at the new order.
    let mut order: Vec<usize> = (0..usages.len()).filter(|&i| usages[i].total_bytes() > 0).collect();
    order.sort_by(|&a, &b| {
        usages[b].total_bytes().cmp(&usages[a].total_bytes()).then_with(|| usages[a].name.cmp(&usages[b].name))
    });
    let mut new_index = vec![u32::MAX; usages.len()];
    for (new, &old) in order.iter().enumerate() {
        new_index[old] = new as u32;
    }
    for owner in owners.iter_mut() {
        if let Some(o) = owner {
            match new_index[o.app as usize] {
                u32::MAX => *owner = None, // an app whose folders are all empty
                n => o.app = n,
            }
        }
    }
    cache_dirs.sort_unstable();
    cache_dirs.dedup();
    Attribution {
        apps: order.into_iter().map(|i| usages[i].clone()).collect(),
        owners,
        cache_dirs,
    }
}

fn descend(tree: &ScanTree, names: &[String], matcher: &Matcher, dir: u32, rest: &[&str], hits: &mut Vec<u32>) {
    let Some((first, rest)) = rest.split_first() else {
        hits.push(dir);
        return;
    };
    for &child in &tree.dirs[dir as usize].children {
        if matcher.eq(first, &names[child as usize]) {
            descend(tree, names, matcher, child, rest, hits);
        }
    }
}

fn is_programs_folder(path: &Path, env: &Env, matcher: &Matcher) -> bool {
    let text = path.to_string_lossy();
    let parts = split_path(&text);
    let home = split_path(&env.home);
    let is = |candidate: &[&str]| {
        candidate.len() == parts.len() && candidate.iter().zip(&parts).all(|(c, p)| matcher.eq(c, p))
    };
    match env.os {
        Os::MacOs => {
            let mut user_apps = home.clone();
            user_apps.push("Applications");
            is(&["Applications"]) || is(&user_apps)
        }
        Os::Windows => {
            let drive = env.system_drive.as_str();
            let mut user_programs = home.clone();
            user_programs.extend(["AppData", "Local", "Programs"]);
            is(&[drive, "Program Files"]) || is(&[drive, "Program Files (x86)"]) || is(&user_programs)
        }
        Os::Linux => false,
    }
}
