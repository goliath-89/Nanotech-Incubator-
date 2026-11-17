# Strata

A desktop app for Windows and macOS that shows what is using a drive: by kind
of file, by folder, and by the application that owns it.

- Product requirements and plan: https://claude.ai/artifact/MXLptov6Q2eUXVUG74Riq6
- Design concept: https://claude.ai/artifact/R3bpVchpxHeN62tJTk9LmV
- How the code is organised and the contracts between parts: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- What gets built each day: [docs/BUILD_PLAN.md](docs/BUILD_PLAN.md)
- What needs you: [docs/BACKLOG.md](docs/BACKLOG.md)

## What is here

| Crate | Does |
| --- | --- |
| `strata-core` | Walks a folder tree in parallel and totals space on disk |
| `strata-mft` | Scans a whole NTFS volume from its file table (Windows fast scan) |
| `strata-classify` | Kinds of files, and which app each folder belongs to (`rules/*.toml`) |
| `strata-store` | Saves scans in SQLite; growth between scans |
| `strata-safety` | Decides what may be moved to the Trash, and moves it |
| `strata-reclaim` | Cleanup suggestions and duplicate finder (read-only) |
| `strata-cli` | The `strata` command, for testing all of the above |

The desktop app is in `app/` (Tauri 2 + Svelte 5):

```
cd app && npm ci
npx tauri dev          # run the app
npm run dev            # interface only, in a browser, with example data
npx tauri build        # installers
```

On Linux the app needs the webview libraries (`libwebkit2gtk-4.1-dev`).
CI attaches Windows and macOS installers to every run.

## Try it

```
cargo run --release -p strata-cli -- scan /path/to/folder
```

Prints the space used on disk, usage by kind and by app, the largest folders
and files, and how the total compares with the operating system's figure.

| Add | To |
| --- | --- |
| `--reclaim` | list what could be cleaned up (nothing is removed) |
| `--duplicates` | list identical large files |
| `--errors` | list what could not be read |
| `--save history.sqlite` | keep the result; `strata history` and `strata growth` read it back |
| `--fast` | read the NTFS file table: `strata scan C: --fast` as administrator on Windows |
| `--json` | one line for scripts |

`strata make-tree <folder> <files>` and `strata bench <folder>` measure speed.

## Develop

```
cargo test --all
cargo clippy --all-targets -- -D warnings
```

CI runs both on Linux, macOS and Windows and attaches a `strata` build for
each to the workflow run.
