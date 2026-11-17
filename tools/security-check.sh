#!/usr/bin/env bash
# Run Strata's security checks locally. The same checks run in CI
# (.github/workflows/security.yml); CodeQL only runs there.
# Tools: cargo-deny (cargo install --locked cargo-deny), npm, python3,
# gitleaks (optional; https://github.com/gitleaks/gitleaks).
set -uo pipefail
cd "$(dirname "$0")/.."
failed=()
step() { echo; echo "== $1"; }
run() { "$@" || failed+=("$*"); }

step "Rust dependencies: engine"
run cargo deny check advisories bans licenses sources
step "Rust dependencies: app"
run cargo deny --manifest-path app/src-tauri/Cargo.toml check advisories bans licenses sources
step "Interface dependencies"
(python3 tools/npm-audit-gate.py app && cd app && npm audit signatures) || failed+=("npm audit")
step "App hardening"
run python3 tools/check-app-hardening.py
step "Security lints"
run cargo clippy --all-targets -q -- -D warnings -D clippy::undocumented_unsafe_blocks
step "Secrets"
if command -v gitleaks >/dev/null; then run gitleaks git --redact .; else echo "gitleaks not installed; CI runs it"; fi
step "Safety and parser tests"
run cargo test -q -p strata-safety -p strata-mft

echo
if [ ${#failed[@]} -eq 0 ]; then echo "All security checks passed."; else printf 'FAILED:\n'; printf '  %s\n' "${failed[@]}"; exit 1; fi
