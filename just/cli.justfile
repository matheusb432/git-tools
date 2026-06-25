set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

[private]
_install := 'scripts/install.sh'

_default:
    @just --list cli

# Build the release CLI engine + the offline diff-preview bundle: target/release/git-tools[.exe].
[group('cli')]
build: build-js
    cargo build --release

# Place the prebuilt git-tools binary + gtl alias on PATH (~/.local/bin). Build first with `just cli build`.
[group('cli')]
install:
    bash "{{ _install }}" install-cli

# Build + install the CLI engine only (git-tools + gtl). Leaves the desktop viewer untouched.
[group('cli')]
update: build install

# NODE_ENV=production forces frontend dependencies' production builds.
# Bundle frontend/diff -> src/embedded/generated/preview.js (committed). Needs bun.
[group('cli')]
build-js:
    if command -v bun >/dev/null 2>&1; then \
      NODE_ENV=production bunx vite build --config frontend/diff/vite.config.mjs; \
      echo "built src/embedded/generated/preview.js"; \
    else echo "bun not installed; skipping build-js (commit embedded/generated unchanged)" >&2; fi

# Run the TypeScript unit tests for the diff artifact frontend. Needs bun.
[group('cli')]
test-js:
    if command -v bun >/dev/null 2>&1; then bun test --isolate frontend/diff; else echo "bun not installed; skipping test-js" >&2; fi

# Cross-build the Win11 CLI exe from this Linux host via cargo-xwin. Non-authoritative: proves
# linkage, not runtime — certify on real Win11 with `just win-release-checklist`.
# -> target/x86_64-pc-windows-msvc/release/git-tools.exe
[group('cli')]
win-build: build-js
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/win-preflight.sh
    gtl_win_preflight
    cargo xwin build --release -p git-tools --target x86_64-pc-windows-msvc
    echo "built target/x86_64-pc-windows-msvc/release/git-tools.exe"
