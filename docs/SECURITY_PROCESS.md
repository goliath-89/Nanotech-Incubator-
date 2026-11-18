# Security process

How every version of Strata is checked, and what happens when something is
found. No process can promise zero vulnerabilities; this one is built to
find known problems automatically, find new ones early, and fix them on a
clock.

## What Strata must protect

| Asset | Threat | Main defence |
| --- | --- | --- |
| The user's files | Strata removes something it should not | Only `strata-safety` can remove anything, only to the Trash, after a policy check at the real path (links resolved). Enforced by `tools/check-app-hardening.py` |
| The user's computer | Malformed data on disk (a damaged or hostile NTFS volume) makes Strata misbehave | All on-disk structures parsed as untrusted, bounds-checked; random-damage test (`strata-mft`); `unsafe` forbidden in five crates and documented everywhere else |
| The app window | Injected script gains access to Strata's commands | Strict Content Security Policy, no remote content, only Strata's own commands exposed (`core:default` capability) |
| The build | A compromised dependency or CI step | Lock files, `cargo-deny`, `npm audit` + signature checks, CI actions pinned to exact commits, least-privilege CI tokens, Dependabot |
| Privacy | File names leave the computer | No network access in the app (CSP blocks it); telemetry opt-in and counts only (Day 69) |
| Updates | A fake update installs malware | Signed builds and a signed auto-updater (Days 74–75) |

## Checks and when they run

| Check | Tool | Every push and PR | Every Monday | Before a release |
| --- | --- | --- | --- | --- |
| Known vulnerabilities in Rust crates (both workspaces) | `cargo-deny` advisories (RustSec database) | Yes | Yes | Yes |
| Yanked crates, licences, unknown sources | `cargo-deny` bans, licenses, sources | Yes | Yes | Yes |
| Known vulnerabilities in npm packages | `npm audit --audit-level=low` | Yes | Yes | Yes |
| npm package signatures and provenance | `npm audit signatures` | Yes | Yes | Yes |
| Secrets committed anywhere in history | gitleaks | Yes | Yes | Yes |
| Code analysis (Rust, TypeScript, CI workflows) | CodeQL, `security-extended` queries | Yes | Yes | Yes |
| App hardening rules | `tools/check-app-hardening.py` | Yes | Yes | Yes |
| Undocumented `unsafe`, all lints as errors | clippy on Linux, macOS, Windows | Yes | — | Yes |
| Safety engine: no path reaches a protected place | 2,000-case randomised tests per rule (`strata-safety`) | Yes | — | Yes |
| Parser robustness | 400-round random-damage test (`strata-mft`) | Yes | — | Yes |
| Dependency updates | Dependabot pull requests | — | Yes | — |
| Manual review of the security boundary | Fable review day (Days 20, 40, 60, 65, 80, 87) | — | — | Yes |

Run everything that can run locally with `tools/security-check.sh`.

The checks live in `.github/workflows/security.yml`. A failing check blocks
merging once branch protection is turned on (see "Your part" below).

## When something is found

| Severity (CVSS) | Example | Fixed within |
| --- | --- | --- |
| Critical (9.0+) | Remote code execution; Strata removes user files | 48 hours; release paused until fixed |
| High (7.0–8.9) | Local privilege gain; webview escape | 7 days |
| Medium (4.0–6.9) | Denial of service from a crafted volume | 30 days |
| Low (< 4.0) | Information shown that should not be | Next planned release |

Steps, every time:

1. **Confirm** it affects Strata: is the vulnerable code reachable from our
   use of the dependency?
2. **Fix**: upgrade the dependency (`cargo update -p <crate>` or
   `npm update <package>`), or change our code. Add a test that fails
   without the fix where possible.
3. **If no fix exists yet** and the code is not reachable, add an exception
   in `deny.toml` (`ignore = [{ id = "...", reason = "...; review by YYYY-MM-DD" }]`)
   with a review date at most 30 days away; for npm packages, in
   `tools/npm-audit-exceptions.json` (checked by `tools/npm-audit-gate.py`,
   which fails again once the date passes). Exceptions are never added for
   Critical or High advisories that are reachable.
4. **Record** it in the table below.
5. **Release** a fixed build if the problem shipped.

## App hardening rules

Checked by `tools/check-app-hardening.py` on every push:

- Content Security Policy: `default-src 'self'`, scripts only from the app
  itself, no `unsafe-eval`, no `unsafe-inline` scripts, no remote origins.
- No remote content in any window; no devtools in release builds;
  `withGlobalTauri` off; no `dangerous*` settings.
- Windows created in Rust (`WebviewWindowBuilder`) follow the same rules: the
  check scans the app's Rust source for devtools and remote URLs.
- Capabilities: only `core:default`; new permissions are added to the
  script's allow list after review.
- Only `strata-safety` depends on `trash`; no other code calls
  `remove_file`, `remove_dir` or `remove_dir_all`. The one exception is a
  `remove_file` of a file Strata itself just created, marked on the same
  line with `// own-file:` (the temporary result of the Windows fast scan).
- The Windows fast scan runs in a separate elevated copy of Strata, never
  in the window. That copy accepts exactly one drive letter and one output
  file named `fast-scan-<digits>.sqlite` in Strata's own data folder, which
  it creates fresh (never following or overwriting an existing file).
  Before writing it holds the folder open (so it cannot be renamed or
  replaced), refuses links and junctions, and checks the real path of the
  folder and the file through their handles. SQLite runs in memory there
  and the bytes are written once, so no `-wal`, `-shm` or journal file is
  ever created by an administrator process.
- The window names things by id or number only: drives by id (the path to
  scan comes from the system's drive list), folders by id, files by a bare
  name (no separators, drive or stream prefix), suggestions by number.
- One scan at a time.
- Preview never runs a file or opens it in another program: Quick Look on
  macOS, otherwise images (decoded by the web view) and text only; links
  are not followed and cloud-only files are not read.
- `unsafe` is forbidden in `strata-store`, `strata-safety`,
  `strata-classify`, `strata-reclaim` and `strata-pipeline`; every other
  `unsafe` block carries a `// SAFETY:` comment.

## Before each release

- [ ] All checks above green on the release commit.
- [ ] No open Critical or High findings; every `deny.toml` exception still
      within its review date.
- [ ] Software bill of materials generated and attached (`cargo cyclonedx`,
      `npm sbom`) (from Day 75).
- [ ] Builds signed and notarised (from Day 75).

## Findings log

| Date | Finding | Severity | Fix |
| --- | --- | --- | --- |
| 2026-11-17 | Bundled SQLite before 3.54.0 can return uninitialised memory when opening a crafted database and `-wal` pair (RUSTSEC-2026-0459, CVE-2026-119375). Strata only opens its own history file, so exposure needed a local attacker, but it is fixed regardless | Medium | `rusqlite` 0.32 → 0.41 (SQLite 3.54.1); minimum Rust 1.89 |
| 2026-11-17 | `proc-macro-error` unmaintained, via the GTK stack | Informational | Linux-only build-time crate, not in any Windows or macOS build; policy scoped to shipping platforms |
| 2026-11-17 | Strata's own crates had a non-standard licence field and could in theory be published | Low | Marked `publish = false` with a `LicenseRef-Proprietary` licence |
| 2026-11-17 | CI actions referenced by movable tags; default token permissions | Low | Every action pinned to a commit; `contents: read` by default; checkout without stored credentials |
| 2026-11-17 | First CI run: CodeQL could not report (missing `actions: read`); Rust 1.100 lint on manual byte chunking | — (process) | Permission added to the CodeQL job only; three spots changed to fixed-size chunks |
| 2026-11-17 | CodeQL upload refused: code scanning is not switched on for the repository | — (process) | CodeQL now writes its results to a file and `tools/codeql-gate.py` fails the job on any finding; results are kept as a run artifact. Works with or without GitHub code scanning |
| 2026-11-17 | Day 20 review: the elevated fast-scan copy wrote into a folder in the user's profile by path. Anything running as the user could replace that folder (or plant SQLite's `-wal`/`-shm` files) with links to system locations and have an administrator process write there | High | Folder held open and checked through its handle (no links, real path matches); file created without following links and checked the same way; SQLite in memory, bytes written once. Exit code 7 if the folder is not safe; the app then walks the folders |
| 2026-11-17 | Day 20 review: `start_scan` accepted any path from the window | Medium | Takes a drive id only; the path comes from the system's drive list; network drives refused |
| 2026-11-17 | Day 20 review: "Show in Explorer" accepted a file name such as `C:x`, which on Windows replaces the whole path | Medium | Only a single plain name: no separators, no `:` |
| 2026-11-17 | Day 20 review: Clean Up carried on if protecting Strata's own data folder failed | Medium | Fails closed with a message |
| 2026-11-17 | Day 20 review: any number of scans could be started at once | Low | One scan at a time |
| 2026-11-17 | Day 20 review: Content Security Policy did not set `base-uri`, `form-action`, `object-src`, `frame-ancestors` | Low | All set to `'none'` |
| 2026-11-17 | Day 20 review: the per-user install puts Strata's program files where the user can write; malware running as the user could swap the exe before the administrator prompt for the fast scan | Medium — **accepted until Day 75** (review 2026-12-17) | The prompt shows an unverified publisher until signing. Day 75: signed builds, and the elevated copy refuses to run unless its own signature checks out; consider a per-machine install |
| 2026-11-17 | Dependency updates (Dependabot PRs #2–#4): windows-sys 0.61, rusqlite 0.43 (SQLite 3.5x), toml 1.x; checkout, upload-artifact, setup-node v7; gitleaks-action v3 | — (maintenance) | Applied together on the branch; every action SHA checked against its release tag |
| 2026-11-17 | picomatch ReDoS (GHSA-fc5q-39xc-vjpf), via vite and svelte-check; no fixed release yet | High (advisory) — not reachable: build tools only, our own patterns, not in the app | **Accepted until 2026-12-01** in `tools/npm-audit-exceptions.json`; update as soon as a fix ships |
| 2026-11-17 | Repository made public | — (process) | History checked: no secrets (gitleaks, manual grep), no personal email addresses in commits |
| 2026-11-17 | Days 46–47: what Clean Up may empty was widened by 11 cache locations (Brave, Vivaldi and Opera caches; NVIDIA and AMD shader caches; JetBrains, Unity and Go build caches) | — (reviewed change) | Each is rebuilt by its owner and holds nothing the person made; still only via the review step and the Trash. The contract test keeps every app "cache" rule inside what the safety policy allows |
| 2026-11-18 | UAT bug: Clean Up offered Windows Update's download folder, which only administrators may change; the move failed and the failure was easy to miss | Low (no harm: nothing moved) | A read-only access check (open with delete rights, then close) marks such items "protected"; they cannot be ticked, `clean_up` refuses them again, and a failed cleanup is shown as such. Strata still never asks for admin rights to delete |
| 2026-11-18 | New commands: show or preview a Clean Up item, open Disk Cleanup | — (reviewed change) | Items are named by number; the path comes from the scan history, never the window. Preview goes through the existing checks (no links, no cloud-only files). Disk Cleanup is a fixed program with no arguments |
| 2026-11-18 | Duplicates tab: reads large personal files to compare them; can move copies to the Recycle Bin | — (reviewed change) | Files only read (links not followed, cloud-only files never read). Copies are named by number; paths come from the app's own search result. The app, not the window, keeps at least one copy of each set and refuses copies the safety rules protect; every move goes through the safety engine and the Recycle Bin |

## Your part

These are repository settings only the owner can change (Settings on GitHub):

- [ ] Code security → turn on **Dependabot alerts**, **Dependabot security
      updates**, **Secret scanning** and **Push protection**.
- [ ] Branches → add a rule for the main branch: require the CI and
      Security checks to pass, require a pull request before merging.
- [ ] Code security → **Private vulnerability reporting** on.
