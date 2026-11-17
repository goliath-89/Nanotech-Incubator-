# Strata build plan

One row per working day (about 5 hours), 5 days a week, 18 weeks. Start each
day by opening a session in this repo and saying **"Day N"**; the row tells
Claude what to build and tells you what only you can do.

- **Model** is the one to select for that day. Fable days are the
  correctness-critical ones and are packed into the first two weeks. If Fable
  stops being available, run those days on Opus at the highest effort and say
  so at the start of the session, so the work gets an extra review pass.
- **You** lists what needs a person: running builds on your real Mac and
  Windows PC (Claude's cloud machine is Linux), paying for accounts, and
  decisions. A dash means nothing is needed from you that day.
- A gate is a pass/fail test. Do not start the next phase until it passes;
  if it fails, the next day is spent on the failure and later days shift.

## Progress

Updated every time a day is done. Half-done days count as half.

| Scope | Done | Note |
| --- | --- | --- |
| **Overall (90 days)** | 22 / 90 (24%) | Ahead of plan in features: the MVP (planned for weeks 5–8) works on Windows in week 3 |
| Sprint 1 · engines | 8 / 10 (80%) | Days 3, 6 and 10 wait for real-machine runs (Mac reinstall) |
| **Sprint 2 · MVP** | 8 / 10 (80%) | Days 11–15 and 18–20 done; 16 waits for the MacBook, 17 for your MVP notes; MVP installed and both scan methods work on Windows ARM64 |
| **Sprint 3 · ring polish (current)** | 5 / 10 (50%) | Days 21–24 and 26 done; 25 waits for your notes |

Log (newest first):
- 17 Nov · Day 26 ✅ sizes setting (macOS / Explorer / GiB) and locale number formats. Sprint 3 5/10 (50%), overall 22/90 (24%).
- 17 Nov · Day 24 ✅ tooltips and shared kind selection. Sprint 3 4/10 (40%), overall 21/90 (23%).
- 17 Nov · Day 23 ✅ design tokens; Mica sidebar on Windows 11. Sprint 3 3/10 (30%), overall 20/90 (22%).
- 17 Nov · Day 22 ✅ ring fills in live during a scan. Sprint 3 2/10 (20%), overall 19/90 (21%).
- 17 Nov · Day 21 ✅ ring at 60 fps with 100k folders, spring animation. Sprint 3 1/10 (10%), overall 18/90 (20%). Name decided: Strata.
- 17 Nov · Day 20 ✅ bridge and app review: 1 high, 4 medium, 2 low fixed; dependencies updated. Sprint 2 8/10 (80%), overall 17/90 (19%).
- 17 Nov · Day 19 ✅ 23 UI tests in CI (Playwright, example data). Sprint 2 7/10 (70%), overall 16/90 (18%).
- 17 Nov · Day 18 ✅ nightly accuracy and benchmark CI on all three systems. Sprint 2 6/10 (60%), overall 15/90 (17%).
- 17 Nov · Day 15 ✅ MVP: elevated fast scan, Recycle Bin item. Sprint 2 5/10 (50%), overall 14/90 (16%). CodeQL gate fixed; all checks green.
- 17 Nov · Days 13–14 ✅ Apps, Largest Files, Clean Up tabs; security process in CI. Sprint 2 4/10 (40%).
- 17 Nov · Days 11–12 ✅ app shell and ring. Sprint 2 2/10 (20%).
- 17 Nov · Gate A passed on Windows (fast scan 99.8%, 3.8 s). Sprint 1 8/10 (80%).

Next: Day 27 (keyboard navigation of the ring and lists), Day 28 ("not scanned" and cloud-only as their own segments), Day 29 (ring UI tests). Days 25 and 17 wait for your MVP notes; Day 16 for the MacBook.

## Sprint 1 · weeks 1–2 · scanner and the risky engines

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 1 ✅ | Fable | Workspace, scanner core (parallel walk, size on disk, hard links, cloud files, unreadable folders, cancel, progress), `strata scan` command, 16 tests, CI for 3 platforms, architecture contract | — |
| 2 ✅ | Fable | `strata-store`: SQLite schema, save / load / children / diff / prune, all-or-nothing saves, 14 tests; `strata scan --save`, `strata history`, `strata growth` | Download the `strata` builds from the CI run. Run `strata scan / --errors` on your Mac and `strata scan C:\ --errors` on Windows. Paste both outputs. |
| 3 | Fable | Fix what Day 2's real runs show. macOS: Data volume and firmlinks, snapshots, purgeable space, clone gap. Windows: cluster and small-file rules. Windows test suite | Rerun both scans, paste outputs. Note the used space Finder and Explorer show. |
| 4 ✅ | Fable | `strata-safety`: protected paths for both systems, move to Trash / Recycle Bin, 28 tests including randomized ones that no spelling of a path reaches a protected place | — |
| 5 ✅ | Fable | `strata-mft`: reads NTFS file records, sizes from run lists, names, hard links; builds the same `ScanTree`; 9 tests on a real NTFS image, total matches the volume's used space to the byte | — |
| 6 ◐ | Fable | Done: `strata scan C: --fast`, `NotNtfs` and `NeedsAdministrator` errors for falling back. Left: fixes from your first real run, shadow copies under System, hard-link lookups for the plain walk | Run `strata.exe scan C: --fast --errors` as administrator and `strata.exe scan C:\ --errors` normally; paste both. |
| 7 ✅ | Fable | `strata-classify`: extension table (about 600), folder-context kinds, TOML rule format and loader, attribution engine, 20 starter apps per system, 11 tests | — |
| 8 ✅ | Fable | Benchmark tool: `strata make-tree` and `strata bench`. On Linux (4 cores, warm cache): 2M files in 1.7 s, 107 MB peak. Mac and Windows tuning waits for your numbers | Run the 2M benchmark on both machines (about 10 minutes each), paste the `RESULT` lines. |
| 9 ✅ | Fable | `strata-reclaim`: duplicate finder (size, partial hash, full hash) and cleanup suggestion engine on top of the safety rules, 8 tests; `strata scan --reclaim --duplicates` lists them | — |
| 10 ◐ | Fable | Review done, 7 findings fixed. **Gate A passed on Windows** (fast scan 99.8%, 3.8 s). Mac part waits for the reinstalled MacBook | **Gate A on Mac:** scan and benchmark once the MacBook is set up again. |

### Day 10 review: what was found and fixed

| Finding | Fix |
| --- | --- |
| Personal folders named `Applications`, `Temp`, `cache` or `Pods` were classed as apps, cache or developer files | Those names no longer count inside Documents, Pictures and the like; programs folders only count at the top of a drive or home folder |
| The duplicate finder would have read cloud-only files, making OneDrive or iCloud download them | Files that take no space on this disk are never read |
| On a drive in use, files deleted mid-scan showed up as "could not be read" | Vanished files are skipped silently; a test scans while another thread creates and deletes files |
| The bridge contract promised a drive list that no crate provided | `strata_core::volumes()` for macOS, Windows and Linux, with stable ids; `strata volumes` |
| App attribution and cleanup suggestions were computed but could not be saved with a scan | `save_apps` / `apps` / `save_suggestions` / `suggestions` in the store; `--save` stores all three |
| Finding apps that no rule names built a full path for every folder of the scan | Only the children of the programs folders are looked at |
| The safety engine accepted Windows device names (`NUL`, `CON`, `COM1`) as file names | Refused, in any folder and with any extension |

Still open after the review, all needing real machines: everything in the
"Known gaps" table of ARCHITECTURE.md, the first run of the fast scan on a
real Windows drive, and benchmark numbers for Mac and Windows.

### First real Windows run (folder walk, C:, 475 GB drive)

| Finding | Status |
| --- | --- |
| Total matches Windows exactly: 441,209,839,616 bytes used | Confirmed |
| Walk sees 418 of 441 GB (94.8%); 562 folders are locked even for administrators | Expected; the fast scan reads them. Gate A on Windows is judged on `--fast` |
| Walk took 61 s for 470k files (7,700 files/s), far slower than Linux | Open: rerun warm to separate cold disk cache and AVG's on-access scanning from the walk itself |
| Same folder name in both Program Files folders listed twice (Google, Common Files) | Fixed: merged; Common Files shown as "Shared program files" |
| `~/.docker`, `C:\Program Files\WSL` and Claude's VM bundle were not attributed | Fixed: rules added |
| 8.8 GB Recycle Bin counted as System | Open: show it as its own item and offer "Empty Recycle Bin" through Windows itself |
| Error list printed every locked folder (500+ lines) | Fixed: grouped by the folder they are in |

### Fast scan on real Windows (C:, administrator, Snapdragon ARM64 laptop)

| Check | Result |
| --- | --- |
| Total | 440 of 441 GB used (99.8%) in **3.8 s** for 528,678 files. **Gate A passes on Windows** |
| Hard links | 78,099 extra links (25.3 GB) counted once; the walk counts them twice and reaches 105% |
| Walk, warm, administrator | 7.8 s for the same files; the earlier 61 s was a cold cache without administrator rights |
| "Cloud-only 321 GB" | Bug, fixed: the on-disk "has extended attributes" bit (set by WSL and Docker) was read as "recall on open" |
| WindowsApps reported as one 30 GB app | Fixed: each Store package is its own app, versions merged |
| Edge's program files mostly counted under the vendor folder | Fixed: EdgeCore and EdgeUpdate belong to Edge; WebView2 is its own entry |
| The laptop is ARM64 and ran the x64 build under emulation | CI now also builds `strata-cli-windows-arm64` |
| Still open | Walk without administrator double-counts hard links (the app should ask for elevation and use the fast scan); Recycle Bin shown as System |

## Sprint 2 (revised) · MVP first

The engines finished early, so the app is built around a usable MVP first
and polished after. Sprints 3 and 4 below now polish the same views instead
of introducing them.

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 11 ✅ | Opus | Tauri 2 app: drive list, Scan with live progress and Stop, kind bar, largest folders with click-to-open and breadcrumb, reopen last scan instantly. Shared `strata-pipeline` crate so app and command line run the same steps. CI builds installers for Windows x64, Windows ARM64 and macOS | Install the Windows ARM64 build from Actions and try it. Note anything confusing. |
| 12 ✅ | Opus | The ring: three levels on Canvas, slices under 0.4% folded into "smaller folders", hover shows the slice in the centre, click a slice to open it (any level), click the centre to go back, ring and list highlight each other, unfold animation, light and dark | — |
| 13 ✅ | Opus | Tabs: Folders, Apps (per app: program / data / cache, expand to see where, open or show in Explorer/Finder), Largest Files (filter by kind), Show in Explorer / Finder everywhere | — |
| 14 ✅ | Opus | Clean Up tab: suggestions by group (rebuildable ones pre-ticked, downloads not), review step listing every path, Move to Recycle Bin / Trash through the safety engine, result with anything left in place. The window can only name saved suggestions by number | Try a real cleanup on something you don't need. |
| 15 ✅ | Opus | Windows: ask for administrator rights to use the fast scan; Recycle Bin shown as its own item. **MVP** | Use the MVP for a few days; list what to fix. |
| 16 | Opus | macOS: Full Disk Access check and setup screen; sidebar vibrancy (needs the private-API switch: no Mac App Store build while it is on) | Once the MacBook is ready: install and scan. |
| 17 | Opus | Fixes from your MVP notes | — |
| 18 ✅ | Opus | Nightly CI (`.github/workflows/nightly.yml`, 02:23 UTC and on demand): a known 100k-file tree must match an independent count exactly on all three systems (hard links included); the whole system drive must be 97% (Linux walk) or 98% (Windows fast scan) covered and never over 100.5%; macOS reported only until Day 3. Benchmark with speed and memory limits per system (`tools/bench-baseline.json`); Windows peak memory now measured | — |
| 19 ✅ | Opus | 23 UI tests (`app/tests`, `npm run test:ui`, in CI): drives, scan progress, Stop, administrator and file-table messages, folder order and shares, Trash by name, breadcrumb and keyboard back, ring hover/click/centre, Apps, Largest Files filter, Clean Up ticks/review/cancel/move, Trash card, Windows wording, dark mode, no sideways scrolling at 800 and 1440 px, keyboard use | — |
| 20 ✅ | Opus | Review of the bridge and app: 7 findings (1 high, 4 medium, 2 low) fixed, 1 accepted until signing on Day 75 (see SECURITY_PROCESS.md). All Dependabot updates applied and verified | ✅ Name decided: **Strata** |

## Sprint 3 · weeks 5–6 · the ring (polish)

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 21 ✅ | Opus | Ring drawn once into an image (fills batched by colour), hover redraws only the lit slice, hit-testing by binary search. With a 100,000-folder tree: built in ~8 ms once, frames under 2.1 ms, 61–62 fps while hovering (4 performance tests in CI). Spring animation: opens from the slice clicked, settles back when going up; off with Reduce Motion | — |
| 22 ✅ | Opus | Ring fills in live during a scan: the scanner keeps lock-free running totals per top-level folder; the app sends them once a second; the ring grows clockwise against the drive's used space, with the uncounted part as a faint track. Totals end exactly at the final figures (2 new engine tests, 1 UI test) | — |
| 23 ✅ | Opus | Design pass: shape and depth tokens (card, control, pill radii; raised shadow) used everywhere; window created in code; **Mica behind the sidebar on Windows 11** (build 22000+, content area stays solid; Windows 10 unchanged), light and dark. macOS vibrancy moved to Day 16: it needs Tauri's private-API switch, which rules out the Mac App Store | Check the Mica sidebar on your laptop (see BACKLOG). |
| 24 ✅ | Opus | One shared tooltip (hover after a pause, keyboard focus at once, Escape hides, screen-reader description) on folder rows (full path), kind bar, drives, capacity bar and app bars. Shared selection: pick a kind in the kind bar and the ring fades the rest, folder bars show only that kind, Largest Files opens on it; click again or "Show all kinds" to clear. 4 UI tests | Use it on your real drive for 10 minutes; list what confuses you. |
| 25 | Opus | Fixes from your notes | — |
| 26 ✅ | Opus | Sizes setting (Settings at the foot of the sidebar): like macOS (1 GB = 10⁹ bytes), like Windows Explorer (1 GB = 2³⁰ bytes) or GiB; defaults to the system's own convention and is remembered. Every number, percentage and time uses the system locale (e.g. 1,5 GB and 67,2 % in German). 2 UI tests | — |
| 27 | Opus | Keyboard navigation of the ring and lists | — |
| 28 | Opus | "Not scanned" and cloud-only items shown as their own segments | — |
| 29 | Opus | UI tests for the ring | — |
| 30 | Opus | Buffer day: open bugs | — |

## Sprint 4 · weeks 7–8 · polish and testing

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 31 | Opus | Right-click menu: Reveal, Preview, Copy Path, Exclude | — |
| 32 | Opus | Quick Look on Mac, preview on Windows | — |
| 33 | Opus | Exclusions and theme in Settings | — |
| 34 | Opus | Reduce Motion and high-contrast support | — |
| 35 | Opus | Scan history: pick an older scan, see what grew | — |
| 36 | Opus | Performance pass on very large drives (4M files) | — |
| 37 | Opus | UI tests for everything so far | Recruit 5 people for a 15-minute test next day. |
| 38 | Opus | Usability test script and a build for testers | Run the 5 tests; write down clicks to find the largest folder. |
| 39 | Opus | Fixes from the tests | — |
| 40 | Fable or Opus | Review; **Gate B check** | **Gate B:** testers find their largest folder in 3 clicks or fewer on both systems. |

## Sprint 5 · weeks 9–10 · kinds and app rules

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 41 | Opus | Wire the classifier into scans; By Kind totals in the ring | — |
| 42 | Opus | Bundles as single items (`.app`, `.photoslibrary`, `.fcpbundle`, game folders) | — |
| 43 | Opus | File-signature check for the largest unknown files | — |
| 44 | Opus | App rules 1–25 for macOS with fixtures | — |
| 45 | Opus | App rules 26–50 for macOS | Scan your Mac; paste the 20 largest "Other" folders. |
| 46 | Opus | App rules 1–25 for Windows | — |
| 47 | Opus | App rules 26–50 for Windows | Scan your PC; paste the 20 largest "Other" folders. |
| 48 | Opus | Rules for whatever your "Other" lists showed | — |
| 49 | Opus | Installed-app discovery (LaunchServices, uninstall registry, Store packages) | — |
| 50 | Fable or Opus | Review of attribution: nothing guessed, every rule has a fixture | — |

## Sprint 6 · weeks 11–12 · By App and Map

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 51 | Opus | App rules 51–100 for macOS | — |
| 52 | Opus | App rules 51–100 for Windows | — |
| 53 | Opus | Game libraries: Steam, Epic, GOG, Xbox, Battle.net | — |
| 54 | Opus | By App view: app / data / cache bars, expand to paths | — |
| 55 | Opus | Treemap layout and drawing | — |
| 56 | Opus | Treemap hover, zoom, shared selection with the ring | — |
| 57 | Opus | Search by name across results | — |
| 58 | Opus | UI tests for By App, Map, search | Ask 10 people to run a scan and send the "Other" percentage (the build shows it). |
| 59 | Opus | Rules for the gaps the 10 drives showed | — |
| 60 | Fable or Opus | Review; **Gate C check** | **Gate C:** under 10% of used space is "Other" on the 10 drives. |

## Sprint 7 · weeks 13–14 · Reclaim

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 61 | Opus | `suggestions` and `trash` commands wired to the Day 4 and Day 9 engines | — |
| 62 | Opus | Reclaim view: groups, totals, per-item explanation | — |
| 63 | Opus | Review sheet and Move to Trash / Recycle Bin, with the refused list | — |
| 64 | Opus | Duplicates group in Reclaim | — |
| 65 | Fable or Opus | Safety audit: try to make the UI and bridge trash a protected path; fuzz the `trash` command | — |
| 66 | Opus | Fixes from the audit; Sidebar "Reclaim N GB" entry | Use Reclaim on a test folder, then on real caches. Check the Trash has what you expect. |
| 67 | Opus | Pricing gate: free vs Pro features; licence key check | Decide: one-time $29 or subscription. Open a Paddle or Lemon Squeezy account. |
| 68 | Opus | Landing page with waitlist form and screenshots | Buy the domain; publish the page. |
| 69 | Opus | Opt-in anonymous counts and crash reports | Decide the telemetry provider, or none. |
| 70 | Opus | Buffer day: open bugs | — |

## Sprint 8 · weeks 15–16 · Growth, fast scan, beta

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 71 | Opus | Snapshots saved after each scan; `growth` command | — |
| 72 | Opus | Growth view: top growers and shrinkers, sparklines | — |
| 73 | Opus | Fast scan switch on Windows with the elevation prompt | Test the fast scan in the app on Windows. |
| 74 | Opus | Auto-update through Tauri's updater | Enrol in the Apple Developer Program ($99 / year). Order Windows code signing. |
| 75 | Opus | Signing and notarization in CI | Add the signing secrets to the repository settings (Claude lists exactly which). |
| 76 | Opus | Beta build, feedback link, known-issues page | Invite 20 testers from the waitlist. |
| 77 | Opus | Triage and fix beta reports | Forward tester reports. |
| 78 | Opus | Triage and fix beta reports | Forward tester reports. |
| 79 | Opus | Triage and fix beta reports | Forward tester reports. |
| 80 | Fable or Opus | Review; **Gate D check** | **Gate D:** no tester reports a cleanup removing something needed. |

## Sprint 9 · weeks 17–18 · polish and release

| Day | Model | Claude builds | You |
| --- | --- | --- | --- |
| 81 | Opus | Accessibility pass: VoiceOver and Narrator labels, contrast, focus order | Try VoiceOver or Narrator for 10 minutes. |
| 82 | Opus | Fix every P0 and P1 bug from the beta | — |
| 83 | Opus | Fix every P0 and P1 bug from the beta | — |
| 84 | Opus | Microsoft Store package; Homebrew cask and winget manifest | Create the Microsoft Store listing. |
| 85 | Opus | Final website: screenshots, pricing, download buttons, privacy page | Review the copy. |
| 86 | Opus | Clean-install tests on fresh virtual machines; installer fixes | Install on one clean Mac and one clean PC. |
| 87 | Fable or Opus | Release review of the whole codebase | — |
| 88 | Opus | Fixes from the review; release candidate | Use the release candidate for a day. |
| 89 | Opus | Launch posts drafted (Product Hunt, Show HN, r/macapps, r/Windows11) | **Gate E:** installs with no security warnings; crash-free 99.5% in the last beta week. |
| 90 | Opus | Tag v1.0, publish builds | Post the launch. |

## Decisions that will be asked of you

| When | Decision |
| --- | --- |
| Day 20 ✅ | Final product name: **Strata** |
| Day 67 | One-time price or subscription |
| Day 69 | Telemetry provider, or none |
| Day 74 | Pay for Apple Developer Program and Windows signing |
| After v1.0 | Whether to ship a Mac App Store version |
