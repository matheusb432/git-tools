set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list cli

# Build the release CLI engine + gtl-server: target/release/{git-tools,gtl-server}[.exe].
[group('cli')]
build:
    cargo run --quiet -p xtask -- build --target cli

# Place the prebuilt git-tools binary + gtl alias + gtl-server on PATH (~/.local/bin). Build first with `just cli build`.
[group('cli')]
install:
    cargo run --quiet -p xtask -- install --target cli

# Build + install the CLI engine only (git-tools + gtl + gtl-server). Leaves the desktop viewer untouched.
[group('cli')]
update: build install
