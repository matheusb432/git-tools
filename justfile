set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]

mod cli 'just/cli.justfile'
mod desktop 'just/desktop.justfile'

[private]
_binname := if os() == "windows" { "git-tools.exe" } else { "git-tools" }
[private]
_bin := justfile_directory() / "target" / "release" / _binname

_default:
    @just --list --unsorted

# Print the git-tools command reference.
help: _preflight
    @"{{ _bin }}" --help

# ============ build / install (aggregate over cli + desktop) ============

# Build both the CLI engine (+ diff bundle) and the desktop viewer.
[group('build')]
build:
    cargo run --quiet -p xtask -- build

# Place both the prebuilt CLI engine and desktop viewer on PATH. Build first with `just build`.
[group('build')]
install:
    cargo run --quiet -p xtask -- install --target both

# Build + install everything: the CLI engine (git-tools + gtl + gtl-daemon) and the desktop viewer (gtl-viewer).
[group('build')]
update:
    just build
    just install

# Remove the installed CLI, daemon, alias, viewer, desktop entry, and icon; preserve configuration.
[group('build')]
uninstall:
    cargo run --quiet -p xtask -- uninstall

# Remove installed artifacts and permanently delete repo-local git-tools configuration.
[confirm("Remove installed artifacts and delete git-tools.toml / git-tools.secrets.toml?")]
[group('build')]
purge:
    cargo run --quiet -p xtask -- uninstall --remove-config --force

# Build only if the binary is missing (preflight for run recipes).
_preflight:
    test -x "{{ _bin }}" || cargo build --release -p cli -p daemon

# ============ quality ============

# Fast gate: check (formatting + linters) + default-member Rust tests (desktop excluded). --e2e runs only hermetic viewer E2E; --all adds all Rust tests, frontend, drift, and E2E; --verbose streams logs.
[group('quality')]
test *args:
    cargo run --quiet -p xtask -- test {{ args }}

# Run the complete Rust workspace with LLVM line coverage. The report excludes dedicated test files and writes per-line output under .artifacts/coverage/text/.
[group('quality')]
cov:
    cargo run --quiet -p xtask -- cov

# Apply pinned-nightly rustfmt, Taplo, rumdl, and Oxfmt across the repository. --verbose restores taplo's file-discovery logs.
[group('quality')]
fmt *args:
    cargo run --quiet -p xtask -- fmt {{ args }}

# Check formatting without modifying files (exits non-zero on drift); formatting only. --verbose restores taplo's file-discovery logs.
[group('quality')]
fmt-check *args:
    cargo run --quiet -p xtask -- fmt-check {{ args }}

# Run Oxlint, presentation and architecture policy, dependency checks, and workspace Clippy.
[group('quality')]
lint:
    cargo run --quiet -p xtask -- lint

# Complete read-only quality gate: formatting, lint, and configured ast-grep rules.
[group('quality')]
check:
    cargo run --quiet -p xtask -- check

# Apply Clippy and Oxlint fixes first, then normalize every formatter; extra args go to Clippy.
[group('quality')]
fix *args:
    cargo run --quiet -p xtask -- fix {{ args }}

# Rebuild the committed diff-preview JS bundle and fail if it drifts from its TypeScript sources.
[group('quality')]
drift-check:
    cargo run --quiet -p xtask -- drift-check

# Read-only first-run check for required tools, Linux desktop-test dependencies, and hook wiring.
[group('quality')]
doctor *args:
    doctor-rs {{ args }}

# ============ windows cross-build (host/release split — see specs) ============

# Cross-build all three Win11 exes; runs `just test --all` first unless -f/--force. --smoke selects a debug linkage build; use `--smoke --force` for the fast smoke path.
[group('windows')]
ship *args:
    cargo run --quiet -p xtask -- ship {{ args }}

# Print the manual Win11 runtime-certification checklist (run on a real Windows box/VM).
[group('windows')]
win-release-checklist:
    @cat docs/windows-release-checklist.md

# Full dev-host bring-up: link skills, configure hooks, build + install both artifacts, ensure ~/.local/bin on PATH (fresh machine: `sh xtask/bootstrap.sh`).
bootstrap:
    cargo run --quiet -p xtask -- bootstrap
