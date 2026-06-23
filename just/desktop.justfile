set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

[private]
_install := 'scripts/install.sh'

_default:
    @just --list desktop

# Build the gtl-viewer Tauri binary (skips with a clear message if webkit2gtk-4.1 headers are absent).
[group('desktop')]
build: build-viewer-ui
    if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then \
      cargo build --release -p desktop --features custom-protocol; \
    else echo "webkit2gtk-4.1 headers absent — skipping gtl-viewer build (CLI-only mode; browser fallback active)" >&2; fi

# Place the prebuilt gtl-viewer binary on PATH via atomic mv (warm-tray safe). Build first with `just desktop build`.
[group('desktop')]
install:
    bash "{{ _install }}" install-viewer

# Build + install the desktop viewer only (gtl-viewer). Leaves the CLI engine untouched.
[group('desktop')]
update: build install

# Bundle the viewer Lit shell -> crates/desktop/dist (committed). Needs bun.
[group('desktop')]
build-viewer-ui:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v bun >/dev/null 2>&1; then
      bun install --cwd crates/desktop/ui lit >/dev/null 2>&1 || true
      mkdir -p crates/desktop/dist
      cp crates/desktop/ui/index.html crates/desktop/dist/index.html
      NODE_ENV=production bun build crates/desktop/ui/shell.ts \
        --outfile crates/desktop/dist/shell.js --format=iife --minify --target=browser
      echo "built crates/desktop/dist/shell.js"
    else echo "bun not installed; skipping build-viewer-ui" >&2; fi
