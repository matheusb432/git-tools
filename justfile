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
    just cli build
    just desktop build

# Place both the prebuilt CLI engine and desktop viewer on PATH. Build first with `just build`.
[group('build')]
install:
    just cli install
    just desktop install

# Build + install everything: the CLI engine (git-tools + gtl + gtl-daemon) and the desktop viewer (gtl-viewer).
[group('build')]
update:
    just cli update
    just desktop update

# Remove the installed binary/alias + viewer. Add --remove-config to also delete git-tools.toml/secrets (confirms).
[group('build')]
uninstall *args:
    cargo run --quiet -p xtask -- uninstall {{ args }}

# Build only if the binary is missing (preflight for run recipes).
_preflight:
    test -x "{{ _bin }}" || cargo build --release -p cli -p daemon

# ============ quality ============

# cargo tests (terse). --verbose streams output; --all also runs the Deno frontend tests.
[group('quality')]
test *args:
    cargo run --quiet -p xtask -- test {{ args }}

# Format Rust with the pinned nightly rustfmt and all TOML with taplo (no-op if taplo is absent).
[group('quality')]
fmt:
    cargo run --quiet -p xtask -- fmt

# Check Rust + TOML formatting without writing, then run the architecture lints (check-structure + check-deps; exit 3 on violation).
[group('quality')]
fmt-check:
    cargo run --quiet -p xtask -- fmt --check

# Rebuild the committed diff-preview JS bundle and fail if it drifts from its TypeScript sources (CI/pre-commit gate).
[group('quality')]
drift-check:
    cargo run --quiet -p xtask -- drift-check

# ============ windows cross-build (host/release split — see specs) ============

# Cross-build all three Win11 release exes (CLI + gtl-daemon + viewer) from this Linux host via the xtask `ship` verb; `--smoke` = fast debug linkage check (runtime is certified separately on real Win11 — see `win-release-checklist`).
[group('windows')]
ship *args:
    cargo run --quiet -p xtask -- ship {{ args }}

# Print the manual Win11 runtime-certification checklist (run on a real Windows box/VM).
[group('windows')]
win-release-checklist:
    #!/usr/bin/env bash
    cat <<'EOF'
    git-tools — Windows 11 runtime certification checklist
    (Run on a real Win11 machine/VM. Linux cargo-xwin artifacts prove linkage, not runtime.)

      1. Build/transfer both exes onto PATH:
           just cli build && just cli install   (native Git Bash build), or transfer the
           cross-built target/x86_64-pc-windows-msvc/release/*.exe
      2. In a git repo, run:  gtl diff
           -> the gtl-viewer window opens and WebView2 RENDERS the diff in a tab.
      3. Close the window  -> a tray icon remains (keep-warm). Re-run `gtl diff`
           -> the existing window is raised/focused (single-instance); no second process.
      4. Store path: artifacts land under %LOCALAPPDATA%\git-tools\data\diffs\... ;
           the History panel lists past diffs.
      5. Tray "Quit" terminates the process.
      6. `gtl diff --viewer browser` opens the artifact in the default browser (explorer.exe).
      7. With the viewer absent / no display, `gtl diff` degrades to the browser path
           without erroring.

    All seven green => Windows runtime certified for this build.
    EOF

# Full dev-host bring-up: link skills, build + install both artifacts, ensure ~/.local/bin on PATH (fresh machine: `sh xtask/bootstrap.sh`).
bootstrap:
    cargo run --quiet -p xtask -- bootstrap
