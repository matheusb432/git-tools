set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list desktop

# Start the debug viewer with the isolated development identity and production-shaped embedded assets.
[group('desktop')]
up:
    deno task --frozen build
    cd crates/gtl-desktop && cargo tauri dev --config tauri.dev.conf.json --features custom-protocol --no-dev-server

# Run the removable Dioxus Web shell proof inside its isolated Tauri development identity.
[group('desktop')]
dioxus-poc:
    cd crates/gtl-desktop && cargo tauri dev --config tauri.dioxus.conf.json --features dioxus-poc

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
