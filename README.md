# Strata

A desktop app for Windows and macOS that shows what is using a drive: by kind
of file, by folder, and by the application that owns it.

- Product requirements and plan: https://claude.ai/artifact/MXLptov6Q2eUXVUG74Riq6
- Design concept: https://claude.ai/artifact/R3bpVchpxHeN62tJTk9LmV
- How the code is organised and the contracts between parts: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- What gets built each day: [docs/BUILD_PLAN.md](docs/BUILD_PLAN.md)

## Try the scanner

```
cargo run --release -p strata-cli -- scan /path/to/folder
```

Prints the space used on disk, the largest folders and files, anything it
could not read, and how the total compares with the operating system's own
figure for the volume. `--json` prints one line for scripts.

## Develop

```
cargo test --all
cargo clippy --all-targets -- -D warnings
```

CI runs both on Linux, macOS and Windows and attaches a `strata` build for
each to the workflow run.
