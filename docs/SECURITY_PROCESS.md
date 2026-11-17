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
   with a review date at most 30 days away. Exceptions are never added for
   Critical or High advisories that are reachable.
4. **Record** it in the table below.
5. **Release** a fixed build if the problem shipped.

## App hardening rules

Checked by `tools/check-app-hardening.py` on every push:

- Content Security Policy: `default-src 'self'`, scripts only from the app
  itself, no `unsafe-eval`, no `unsafe-inline` scripts, no remote origins.
- No remote content in any window; no devtools in release builds;
  `withGlobalTauri` off; no `dangerous*` settings.
- Capabilities: only `core:default`; new permissions are added to the
  script's allow list after review.
- Only `strata-safety` depends on `trash`; no other code calls
  `remove_file`, `remove_dir` or `remove_dir_all`.
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

## Your part

These are repository settings only the owner can change (Settings on GitHub):

- [ ] Code security → turn on **Dependabot alerts**, **Dependabot security
      updates**, **Secret scanning** and **Push protection**.
- [ ] Branches → add a rule for the main branch: require the CI and
      Security checks to pass, require a pull request before merging.
- [ ] Code security → **Private vulnerability reporting** on.
