#!/usr/bin/env python3
"""Fail if any CodeQL SARIF file in the given folder reports a finding.

Every result counts, whatever its level: see docs/SECURITY_PROCESS.md.
"""
import json
import pathlib
import sys

folder = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "codeql-results")
files = sorted(folder.glob("*.sarif"))
if not files:
    sys.exit(f"No SARIF files in {folder}: the analysis did not run.")

findings = 0
for f in files:
    for run in json.loads(f.read_text(encoding="utf-8")).get("runs", []):
        for r in run.get("results", []):
            if r.get("suppressions"):
                continue
            findings += 1
            loc = r.get("locations", [{}])[0].get("physicalLocation", {})
            path = loc.get("artifactLocation", {}).get("uri", "?")
            line = loc.get("region", {}).get("startLine", "?")
            msg = r.get("message", {}).get("text", "").splitlines()[0][:200]
            print(f"::error file={path},line={line}::{r.get('ruleId')}: {msg}")

if findings:
    sys.exit(f"CodeQL found {findings} issue(s). Fix them per docs/SECURITY_PROCESS.md.")
print(f"CodeQL: no findings in {len(files)} file(s).")
