set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list cli

# Build the release CLI engine + gtl-daemon + the static artifact stylesheet: target/release/{git-tools,gtl-daemon}[.exe].
[group('cli')]
build:
    cargo run --quiet -p xtask -- build --target cli

# Place the prebuilt git-tools binary + gtl alias + gtl-daemon on PATH (~/.local/bin). Build first with `just cli build`.
[group('cli')]
install:
    cargo run --quiet -p xtask -- install --target cli

# Build + install the CLI engine only (git-tools + gtl + gtl-daemon). Leaves the desktop viewer untouched.
[group('cli')]
update: build install
