#!/usr/bin/env python3
"""Run `npm audit` and fail on any advisory, at any severity, unless it is
listed in tools/npm-audit-exceptions.json with a review date not yet passed.

Usage (from the repository root): python3 tools/npm-audit-gate.py [app-dir]
"""
import datetime
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parent.parent
app = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else root / "app"
listed = json.loads((root / "tools" / "npm-audit-exceptions.json").read_text())["exceptions"]
today = datetime.date.today()

out = subprocess.run(["npm", "audit", "--json"], cwd=app, capture_output=True, text=True, shell=sys.platform == "win32")
report = json.loads(out.stdout or "{}")
if "vulnerabilities" not in report:
    sys.exit(f"npm audit gave no report: {out.stderr.strip()[:500]}")

found = {}
for name, v in report["vulnerabilities"].items():
    for via in v.get("via", []):
        if isinstance(via, dict):
            gid = via.get("url", "").rsplit("/", 1)[-1]
            found.setdefault(gid, (via.get("name", name), via.get("severity"), via.get("title")))

problems = []
for gid, (pkg, severity, title) in sorted(found.items()):
    entry = next((e for e in listed if e["id"] == gid), None)
    if entry is None:
        problems.append(f"{gid} {pkg} ({severity}): {title}")
    elif datetime.date.fromisoformat(entry["review_by"]) < today:
        problems.append(f"{gid} {pkg}: exception expired on {entry['review_by']}; review it")
    else:
        print(f"accepted until {entry['review_by']}: {gid} {pkg} ({severity}): {title}")

for e in listed:
    if e["id"] not in found:
        print(f"note: exception {e['id']} ({e['package']}) no longer needed; remove it")

if problems:
    print("npm audit found:")
    for p in problems:
        print("  " + p)
    sys.exit(1)
print(f"npm audit: no unaccepted advisories ({len(found)} accepted).")
