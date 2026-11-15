//! App rules: which folders belong to which application.

use serde::Deserialize;
use strata_core::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// What a folder is to the app that owns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// The program itself.
    App,
    /// What it created and needs.
    Data,
    /// What it rebuilds on its own.
    Cache,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppRule {
    pub name: String,
    pub id: String,
    pub kind: Option<Kind>,
    /// (pattern as written, role)
    pub paths: Vec<(String, Role)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleSet {
    pub apps: Vec<AppRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    app: Vec<RawApp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApp {
    name: String,
    id: String,
    kind: Option<String>,
    #[serde(default)]
    app: Vec<String>,
    #[serde(default)]
    data: Vec<String>,
    #[serde(default)]
    cache: Vec<String>,
}

impl RuleSet {
    /// The rules that ship inside Strata for `os`.
    pub fn builtin(os: Os) -> RuleSet {
        let text = match os {
            Os::MacOs => include_str!("../../../rules/macos.toml"),
            Os::Windows => include_str!("../../../rules/windows.toml"),
            Os::Linux => "",
        };
        RuleSet::from_toml(text).expect("built-in rules are checked by tests")
    }

    /// Parse a rules file. Every mistake is an error naming the app, so a
    /// bad update is rejected whole instead of half-applied.
    pub fn from_toml(text: &str) -> Result<RuleSet, String> {
        let raw: RawFile = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut apps = Vec::with_capacity(raw.app.len());
        for a in raw.app {
            if a.name.trim().is_empty() || a.id.trim().is_empty() {
                return Err("an app has an empty name or id".into());
            }
            let kind = match &a.kind {
                None => None,
                Some(k) => Some(
                    Kind::ALL
                        .into_iter()
                        .find(|c| c.name() == k)
                        .ok_or_else(|| format!("{}: unknown kind {k:?}", a.name))?,
                ),
            };
            let mut paths = Vec::new();
            for (list, role) in [
                (&a.app, Role::App),
                (&a.data, Role::Data),
                (&a.cache, Role::Cache),
            ] {
                for pattern in list {
                    validate_pattern(pattern)
                        .map_err(|why| format!("{}: {pattern:?} {why}", a.name))?;
                    paths.push((pattern.clone(), role));
                }
            }
            if paths.is_empty() {
                return Err(format!("{}: lists no folders", a.name));
            }
            if apps.iter().any(|x: &AppRule| x.id == a.id) {
                return Err(format!("{}: the id {:?} is used twice", a.name, a.id));
            }
            apps.push(AppRule {
                name: a.name,
                id: a.id,
                kind,
                paths,
            });
        }
        Ok(RuleSet { apps })
    }

    /// Add `newer` on top: an app with the same id replaces the old entry.
    pub fn merge(&mut self, newer: RuleSet) {
        for app in newer.apps {
            match self.apps.iter_mut().find(|a| a.id == app.id) {
                Some(slot) => *slot = app,
                None => self.apps.push(app),
            }
        }
    }
}

fn validate_pattern(pattern: &str) -> Result<(), &'static str> {
    let parts = pattern_parts(pattern);
    let anchored = pattern.starts_with("~/")
        || pattern.starts_with('/')
        || (pattern.len() > 3 && pattern.as_bytes()[1] == b':' && pattern.as_bytes()[2] == b'/');
    if !anchored {
        return Err("must start with ~/, / or a drive like C:/");
    }
    if pattern.contains('\\') {
        return Err("must use / between folders");
    }
    if parts.iter().any(|p| *p == "." || *p == "..") {
        return Err("must not contain . or ..");
    }
    // `Name*`, `*.ext` or `*`: one star, at one end.
    if parts.iter().any(|p| {
        p.matches('*').count() > 1 || (p.contains('*') && !p.starts_with('*') && !p.ends_with('*'))
    }) {
        return Err("may only use one * per folder name, at its start or end");
    }
    // A rule must name something specific: at least two real folder names
    // (`Adobe*` counts, a bare `*` does not), or one below the home folder.
    let literal = parts
        .iter()
        .filter(|p| **p != "~" && !p.ends_with(':') && p.trim_matches('*').len() >= 3)
        .count();
    if literal < 2 && !(literal == 1 && pattern.starts_with("~/")) {
        return Err("is too broad");
    }
    if parts.last().is_some_and(|p| *p == "*") {
        return Err("must not end in a bare *");
    }
    Ok(())
}

pub(crate) fn pattern_parts(pattern: &str) -> Vec<&str> {
    pattern.split('/').filter(|p| !p.is_empty()).collect()
}
