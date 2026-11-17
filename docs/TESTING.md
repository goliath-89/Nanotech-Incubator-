# Strata tests

Every push runs all of these in CI (`.github/workflows/ci.yml`); the
nightly run adds accuracy and speed on real runners (`nightly.yml`).

## Engine (Rust, `cargo test --all`, ~100 tests)

| Area | What is checked |
| --- | --- |
| Scanner (`strata-core`) | totals match an independent walk and `du`; hard links counted once; links not followed; sparse files; same result on every run and thread count; running totals per top folder end at the final totals |
| NTFS file table (`strata-mft`) | a real NTFS image: every file and the volume's used space to the byte |
| History (`strata-store`) | saves round-trip exactly; diff, prune, apps, suggestions; in-memory store round-trips through its bytes |
| Safety (`strata-safety`) | protected paths on both systems, including randomised spellings; Trash moves |
| Kinds and apps (`strata-classify`) | extensions, folder context, Recycle Bin/Trash, app rules |
| Cleanup (`strata-reclaim`, `strata-pipeline`) | suggestions, duplicates, cleanup refuses what the policy protects |
| Exclusions (`strata-pipeline`) | removing after a scan equals skipping during the walk; the tree stays well formed |
| App (`app/src-tauri`, Windows in CI) | preview: images, text, links not followed, programs never previewed; base64 |

## Interface (Playwright, `cd app && npm run test:ui`, 65 tests)

Run against the browser build with example data (`src/lib/demo.ts`).

| File | What is checked |
| --- | --- |
| `tests/ui.spec.ts` | drives; scanning, Stop, stage messages, live ring; folders, breadcrumb, Not scanned, cloud-only; ring hover/click/centre; apps; largest files; preview; changes; clean up (ticks, review, cancel, move); Recycle Bin card; Windows wording; right-click menu (mouse and keyboard); exclusions; theme; sizes setting and German number formats; keyboard (ring, lists, tabs); tooltips; shared kind selection; errors and empty states; dark mode; no sideways scrolling at 800 and 1440 px |
| `tests/ring.spec.ts` | slices where their shares say; Reduce Motion; pixel snapshots (light, dark, one kind) |
| `tests/ring-perf.spec.ts` | 100,000-folder ring: frames under 16.7 ms, 50+ fps |
| `tests/a11y.spec.ts` | WCAG contrast in light and dark; Increase Contrast; Windows High Contrast; Reduce Motion |

Approving a deliberate visual change: `npx playwright test ring --update-snapshots`.

## Nightly (`.github/workflows/nightly.yml`)

| Job | Limits |
| --- | --- |
| Accuracy | a known 100k-file tree matches an independent count exactly; the system drive is 97% (Linux) / 98% (Windows fast scan) covered, never over 100.5% |
| Benchmark | files per second and peak memory per system (`tools/bench-baseline.json`) |
| App with a large scan | at 1M files: opening folders and the ring under 0.1 s, comparing under 1 s |
| macOS installer | builds the universal app |

## Security (`.github/workflows/security.yml`)

See `docs/SECURITY_PROCESS.md`.
