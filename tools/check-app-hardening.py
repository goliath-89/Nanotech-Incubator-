#!/usr/bin/env python3
"""Fail if the desktop app's security settings have been weakened.

Checks app/src-tauri/tauri.conf.json, the capability files and Cargo.toml
against the rules in docs/SECURITY_PROCESS.md ("App hardening"). Run from
the repository root. Exit code 1 lists every problem found.
"""
import json
import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parent.parent
app = root / "app" / "src-tauri"
problems = []


def problem(text):
    problems.append(text)


conf = json.loads((app / "tauri.conf.json").read_text())
security = conf.get("app", {}).get("security", {})

# 1. A strict Content Security Policy: only the app's own files.
csp = security.get("csp")
if not csp:
    problem("tauri.conf.json: app.security.csp is missing")
else:
    directives = {d.strip().split()[0]: d.strip().split()[1:] for d in csp.split(";") if d.strip()}
    if directives.get("default-src") != ["'self'"]:
        problem(f"CSP default-src must be exactly 'self', found {directives.get('default-src')}")
    for name, sources in directives.items():
        for s in sources:
            if s in ("*", "http:", "https:", "'unsafe-eval'") or s.startswith("http"):
                problem(f"CSP {name} allows {s}")
        if name == "script-src" and "'unsafe-inline'" in sources:
            problem("CSP script-src allows 'unsafe-inline'")
    if "script-src" in directives and directives["script-src"] != ["'self'"]:
        problem("CSP script-src must be 'self' only")

# 2. No escape hatches.
if security.get("dangerousDisableAssetCspModification"):
    problem("dangerousDisableAssetCspModification is set")
if security.get("freezePrototype") is False:
    problem("freezePrototype is explicitly disabled")
if conf.get("app", {}).get("withGlobalTauri"):
    problem("withGlobalTauri exposes the API to every script; keep it off")
for w in conf.get("app", {}).get("windows", []):
    url = str(w.get("url", ""))
    if url.startswith("http"):
        problem(f"window {w.get('label')} loads remote content: {url}")
    if w.get("devtools"):
        problem(f"window {w.get('label')} enables devtools")
if conf.get("build", {}).get("frontendDist", "").startswith("http"):
    problem("frontendDist points at a remote URL")

# 3. Capabilities: only Strata's own commands and core defaults.
allowed_permissions = {"core:default"}
for cap_file in sorted((app / "capabilities").glob("*.json")):
    cap = json.loads(cap_file.read_text())
    if cap.get("remote"):
        problem(f"{cap_file.name}: grants permissions to remote URLs")
    for p in cap.get("permissions", []):
        name = p if isinstance(p, str) else p.get("identifier", "")
        if name not in allowed_permissions:
            problem(f"{cap_file.name}: permission {name!r} is not on the allow list (add it to this script only after review)")

# 4. No Tauri features that widen what the window can do.
cargo = (app / "Cargo.toml").read_text()
m = re.search(r'^tauri\s*=\s*\{[^}]*features\s*=\s*\[([^\]]*)\]', cargo, re.M)
features = [f.strip().strip('"') for f in (m.group(1).split(",") if m else []) if f.strip()]
for f in features:
    if f in ("devtools", "unstable") or f.startswith("protocol-asset"):
        problem(f"tauri feature {f!r} is enabled")

# 5. Only the safety engine may remove files: no other crate depends on `trash`
#    or calls remove_file / remove_dir_all outside tests.
for manifest in list((root / "crates").glob("*/Cargo.toml")) + [app / "Cargo.toml"]:
    text = manifest.read_text()
    deps = text.split("[dev-dependencies]")[0]
    if re.search(r'^trash\s*=', deps, re.M) and "strata-safety" not in str(manifest):
        problem(f"{manifest.relative_to(root)} depends on `trash`; only strata-safety may")
# A line that removes a file Strata itself just created may say so with an
# `// own-file:` comment; only remove_file qualifies, never a folder.
for src in list((root / "crates").glob("*/src/**/*.rs")) + list((app / "src").glob("**/*.rs")):
    code = src.read_text().split("#[cfg(test)]")[0]
    for number, line in enumerate(code.splitlines(), 1):
        if not re.search(r"\b(remove_file|remove_dir_all|remove_dir)\s*\(", line):
            continue
        if "remove_file(" in line and "// own-file:" in line:
            continue
        problem(f"{src.relative_to(root)}:{number} removes files directly; go through strata-safety")

if problems:
    print("App hardening check FAILED:")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print("App hardening check passed.")
