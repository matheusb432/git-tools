set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list desktop

# Build the gtl-viewer Tauri binary (skips with a clear message if webkit2gtk-4.1 headers are absent).
[group('desktop')]
build:
    if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then \
      cargo build --release -p desktop --features custom-protocol; \
    else echo "webkit2gtk-4.1 headers absent — skipping gtl-viewer build (CLI-only mode; browser fallback active)" >&2; fi

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

# Build and drive the real gtl-viewer binary under Tauri WebDriver (Xvfb + WebKitWebDriver on Linux).
[group('desktop')]
test-e2e:
    cargo run --quiet -p xtask -- desktop-test-e2e
