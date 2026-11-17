#!/usr/bin/env python3
"""Checks for the nightly accuracy and benchmark jobs.

  nightly-check.py tree <folder> <scan.json>
      Count the folder independently of Strata and compare. Files, folders
      and file sizes must match exactly; space on disk too, except on Windows
      where Python cannot read it.
  nightly-check.py volume <scan.json> <min-percent> [--report-only]
      The scan must account for at least <min-percent> of the volume's used
      space, and never more than 100.5% (more would mean double counting).
  nightly-check.py bench <bench-output.txt> <baseline.json> <name>
      Compare the RESULT line with the limits for <name>.
  nightly-check.py app <bench-app-output.txt>
      Browsing a large saved scan must feel instant: opening folders and
      building the ring under 0.1 s, comparing scans under 1 s.

Each check prints a short Markdown table, also appended to the GitHub job
summary when run in Actions, and exits non-zero when it fails.
"""
import json
import os
import stat
import sys


def summary(lines):
    text = "\n".join(lines) + "\n"
    print(text)
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write(text + "\n")


def reference(root):
    """What a plain, single-threaded walk finds (no following symlinks)."""
    files = dirs = logical = alloc = 0
    seen = set()
    has_blocks = hasattr(os.stat_result, "st_blocks")
    root_st = os.lstat(root)
    if has_blocks:
        alloc += root_st.st_blocks * 512
    for here, subdirs, names in os.walk(root, followlinks=False):
        for d in subdirs:
            st = os.lstat(os.path.join(here, d))
            if stat.S_ISLNK(st.st_mode):
                continue
            dirs += 1
            if has_blocks:
                alloc += st.st_blocks * 512
        for n in names:
            st = os.lstat(os.path.join(here, n))
            if not stat.S_ISREG(st.st_mode):
                continue
            # Every name counts as a file; a hard-linked file's bytes count once.
            files += 1
            key = (st.st_dev, st.st_ino)
            if st.st_nlink > 1 and key in seen:
                continue
            seen.add(key)
            logical += st.st_size
            if has_blocks:
                alloc += st.st_blocks * 512
    return {"files": files, "dirs": dirs, "logical_bytes": logical,
            "alloc_bytes": alloc if has_blocks else None}


def check_tree(folder, scan_file):
    scan = json.load(open(scan_file, encoding="utf-8"))
    ref = reference(folder)
    rows = ["| Tree check | Strata | Reference | |", "| --- | ---: | ---: | --- |"]
    ok = True
    for key in ("files", "dirs", "logical_bytes", "alloc_bytes"):
        want = ref[key]
        if want is None:
            rows.append(f"| {key} | {scan[key]:,} | not readable here | skipped |")
            continue
        same = scan[key] == want
        ok &= same
        rows.append(f"| {key} | {scan[key]:,} | {want:,} | {'✅' if same else '❌'} |")
    if scan.get("errors"):
        ok = False
        rows.append(f"| errors | {scan['errors']} | 0 | ❌ |")
    summary(rows)
    return ok


def check_volume(scan_file, min_pct, report_only):
    try:
        scan = json.load(open(scan_file, encoding="utf-8"))
    except (OSError, ValueError) as e:
        summary([f"Volume check: no scan result ({e}). {'⚠️ report only' if report_only else '❌'}"])
        return report_only
    used = scan.get("volume_used_bytes")
    if not used:
        summary(["Volume check: the volume's used space could not be read. ❌"])
        return report_only
    pct = scan["alloc_bytes"] / used * 100
    ok = min_pct <= pct <= 100.5
    mark = "✅" if ok else ("⚠️ report only" if report_only else "❌")
    summary([
        "| Volume check | Value |", "| --- | ---: |",
        f"| Strata total | {scan['alloc_bytes']:,} bytes |",
        f"| Volume used | {used:,} bytes |",
        f"| Coverage | {pct:.2f}% (needs {min_pct}–100.5%) {mark} |",
        f"| Files | {scan['files']:,} in {scan['elapsed_ms'] / 1000:.2f} s |",
        f"| Unreadable | {scan['errors']:,} |",
    ])
    return ok or report_only


def check_bench(output_file, baseline_file, name):
    line = next((l for l in open(output_file, encoding="utf-8")
                 if l.startswith("RESULT ")), None)
    if line is None:
        summary([f"Benchmark {name}: no RESULT line. ❌"])
        return False
    got = dict(kv.split("=", 1) for kv in line.split()[1:])
    limits = json.load(open(baseline_file, encoding="utf-8"))[name]
    files = int(got["files"])
    median = float(got["median_s"])
    rate = files / max(median, 0.001)
    rows = [f"| Benchmark {name} | Value | Limit | |", "| --- | ---: | ---: | --- |"]
    ok = files == limits["files"]
    rows.append(f"| files | {files:,} | {limits['files']:,} | {'✅' if ok else '❌'} |")
    fast_enough = rate >= limits["min_files_per_s"]
    ok &= fast_enough
    rows.append(f"| median | {median:.2f} s ({rate:,.0f} files/s) | ≥ {limits['min_files_per_s']:,} files/s | {'✅' if fast_enough else '❌'} |")
    if got["peak_mb"] == "na":
        rows.append(f"| peak memory | not measured | ≤ {limits['max_peak_mb']} MB | ⚠️ |")
    else:
        peak = float(got["peak_mb"])
        small = peak <= limits["max_peak_mb"]
        ok &= small
        rows.append(f"| peak memory | {peak:.0f} MB | ≤ {limits['max_peak_mb']} MB | {'✅' if small else '❌'} |")
    summary(rows)
    return ok


APP_LIMITS = [("open ", 0.1), ("ring", 0.1), ("largest files", 0.1), ("compare", 1.0)]


def check_app(output_file):
    rows = ["| App with a large scan | Time | Limit | |", "| --- | ---: | ---: | --- |"]
    ok = True
    seen = 0
    for line in open(output_file, encoding="utf-8"):
        parts = line.rsplit(" s ", 1)
        if len(parts) != 2:
            continue
        name, secs = parts[0][:34].strip(), parts[0][34:].strip()
        try:
            t = float(secs)
        except ValueError:
            continue
        limit = next((l for prefix, l in APP_LIMITS if name.startswith(prefix)), None)
        if limit is None:
            rows.append(f"| {name} | {t:.2f} s | | |")
            continue
        seen += 1
        good = t <= limit
        ok &= good
        rows.append(f"| {name} | {t:.2f} s | ≤ {limit} s | {'✅' if good else '❌'} |")
    if seen == 0:
        rows.append("| no timings found | | | ❌ |")
        ok = False
    summary(rows)
    return ok


def main(argv):
    if len(argv) >= 3 and argv[0] == "tree":
        return check_tree(argv[1], argv[2])
    if len(argv) >= 3 and argv[0] == "volume":
        return check_volume(argv[1], float(argv[2]), "--report-only" in argv)
    if len(argv) >= 2 and argv[0] == "app":
        return check_app(argv[1])
    if len(argv) >= 4 and argv[0] == "bench":
        return check_bench(argv[1], argv[2], argv[3])
    sys.exit(__doc__)


if __name__ == "__main__":
    sys.exit(0 if main(sys.argv[1:]) else 1)
