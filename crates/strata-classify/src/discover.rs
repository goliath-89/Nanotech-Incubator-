//! Apps the system says are installed, turned into rules.
//!
//! Windows lists every installed program in the uninstall registry (with
//! the folder it was installed to) and every Store app in the package
//! repository (its data lives in `%LOCALAPPDATA%\Packages\<family>`). These
//! are facts the system records, not guesses, so they can name folders the
//! built-in rules do not know. They only ever name a program or its data:
//! never a cache, so nothing is offered for cleanup on their account.

use crate::rules::{validate_pattern, AppRule, Role, RuleSet};

pub use strata_core::Installed;
pub use strata_core::{folder_of_icon, package_family, usable_folder};

/// Rules for installed apps the built-in rules do not already cover.
pub fn rules_for(installed: &[Installed], builtin: &RuleSet) -> RuleSet {
    let known_names: Vec<String> = builtin.apps.iter().map(|a| a.name.to_lowercase()).collect();
    let known_paths: Vec<String> = builtin
        .apps
        .iter()
        .flat_map(|a| a.paths.iter().map(|(p, _)| p.to_lowercase()))
        .collect();
    let mut out = RuleSet::default();
    for app in installed {
        if app.name.trim().is_empty() || known_names.contains(&app.name.to_lowercase()) {
            continue;
        }
        let mut paths = Vec::new();
        for (folder, role) in [(&app.program, Role::App), (&app.data, Role::Data)] {
            // The same checks as every built-in rule.
            let Some(f) = folder
                .as_deref()
                .and_then(usable_folder)
                .filter(|f| validate_pattern(f).is_ok())
            else {
                continue;
            };
            // A built-in rule already names this folder: it wins.
            if known_paths.contains(&f.to_lowercase()) {
                continue;
            }
            // The same folder twice (two entries of one program).
            if out
                .apps
                .iter()
                .any(|a: &AppRule| a.paths.iter().any(|(p, _)| p.eq_ignore_ascii_case(&f)))
            {
                continue;
            }
            paths.push((f, role));
        }
        if paths.is_empty() || out.apps.iter().any(|a| a.id == app.id) {
            continue;
        }
        out.apps.push(AppRule {
            name: app.name.trim().to_string(),
            id: app.id.clone(),
            kind: app.game.then_some(strata_core::Kind::Games),
            paths,
        });
    }
    out
}

/// What this computer lists as installed. Empty where not supported yet.
pub fn installed_apps() -> Vec<Installed> {
    strata_core::installed_apps()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_rules_win_and_nothing_is_a_cache() {
        let builtin = RuleSet::from_toml(
            r#"
[[app]]
name = "Steam"
id = "Valve.Steam"
app = ["C:/Program Files (x86)/Steam"]
"#,
        )
        .unwrap();
        let installed =
            vec![
            Installed {
                name: "Steam".into(),
                id: "installed:HKLM:Steam".into(),
                program: Some(r"C:\Program Files (x86)\Steam".into()),
                data: None, game: false },
            Installed {
                name: "Krita (x64)".into(),
                id: "installed:HKLM:krita".into(),
                program: Some(r"C:\Program Files\Krita (x64)".into()),
                data: None, game: false },
            Installed {
                name: "Krita again".into(),
                id: "installed:HKLM32:krita".into(),
                program: Some(r"C:\Program Files\Krita (x64)\".into()),
                data: None, game: false },
            Installed {
                name: "Broken".into(),
                id: "installed:HKCU:broken".into(),
                program: Some(r"C:\Program Files".into()),
                data: None, game: false },
            Installed {
                name: "Windows Terminal".into(),
                id: "store:Microsoft.WindowsTerminal_8wekyb3d8bbwe".into(),
                program: None,
                data: Some(
                    r"C:\Users\sam\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe"
                        .into(),
                ), game: false },
        ];
        let rules = rules_for(&installed, &builtin);
        let names: Vec<&str> = rules.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Krita (x64)", "Windows Terminal"]);
        assert!(rules
            .apps
            .iter()
            .flat_map(|a| &a.paths)
            .all(|(_, r)| *r != Role::Cache));
        assert_eq!(rules.apps[1].paths[0].1, Role::Data);
    }

    #[test]
    fn no_two_built_in_apps_claim_one_folder() {
        for os in [crate::Os::Windows, crate::Os::MacOs] {
            let rules = RuleSet::builtin(os);
            let mut seen: std::collections::HashMap<String, &str> = Default::default();
            for app in &rules.apps {
                for (p, _) in &app.paths {
                    if let Some(other) = seen.insert(p.to_lowercase(), &app.name) {
                        assert_eq!(
                            other, app.name,
                            "{p} is claimed by {other} and {}",
                            app.name
                        );
                    }
                }
            }
        }
    }
}
