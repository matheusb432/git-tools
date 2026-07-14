set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list desktop

# Build the gtl-viewer Tauri binary; missing webkit2gtk-4.1 headers fail with an actionable error.
[group('desktop')]
build:
    cargo run --quiet -p xtask -- build --target viewer

# Place the prebuilt gtl-viewer binary on PATH via atomic replace (warm-tray safe). Build first with `just desktop build`.
[group('desktop')]
install:
    cargo run --quiet -p xtask -- install --target viewer

# Build + install the desktop viewer only (gtl-viewer). Leaves the CLI engine untouched.
[group('desktop')]
update: build install

# Render the viewer icon assets (icon.png + multi-res icon.ico) via the Rust xtask generator.
[group('desktop')]
gen-icon:
    cargo run --quiet -p xtask -- gen-icon

# Run the pure viewer-render benchmark with host display variables removed.
[group('desktop')]
bench:
    cargo run --quiet -p xtask -- desktop-bench
