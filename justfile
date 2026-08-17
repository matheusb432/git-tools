set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]

mod cli 'just/cli.justfile'
mod desktop 'just/desktop.justfile'
mod web 'just/web.justfile'

[private]
_binname := if os() == "windows" { "git-tools.exe" } else { "git-tools" }
[private]
_bin := justfile_directory() / "target" / "release" / _binname

_default:
    @just --list --unsorted

# Print the git-tools command reference.
help: _preflight
    @"{{ _bin }}" --help

# Start the debug desktop viewer with watched Rust and CSS sources.
[group('build')]
up:
    just desktop up

# Build both the CLI engine (+ offline artifact runtime) and the desktop viewer.
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
    test -x "{{ _bin }}" || cargo build --release -p gtl-cli -p gtl-daemon

# Run a shared Rust benchmark; --fast selects the concise viewer-render preset.
[group('performance')]
bench *args:
    cargo run --quiet -p xtask -- bench {{ args }}

# Run tests, or use `just test coverage`; coverage defaults to quiet and forwards cargo-llvm-cov arguments.
[group('quality')]
test *args:
    @cargo run --quiet -p xtask -- test {{ args }}

# Apply rustfmt, Taplo, Dioxus RSX, and rumdl across the repository. --verbose restores taplo's file-discovery logs.
[group('quality')]
fmt *args:
    #!/usr/bin/bash
    set -euo pipefail
    cargo fmt
    if [[ "{{ args }}" == *"--verbose"* ]]; then taplo fmt {{ args }}; else RUST_LOG=warn taplo fmt; fi
    dx fmt --package gtl-web --locked
    markdown_files=()
    while IFS= read -r -d '' file; do [[ -f "$file" ]] && markdown_files+=("$file"); done < <(git ls-files --cached --others --exclude-standard -z -- '*.md')
    if ((${#markdown_files[@]})); then rumdl fmt "${markdown_files[@]}"; fi

# Check formatting without modifying files (exits non-zero on drift); formatting only. --verbose restores taplo's file-discovery logs.
[group('quality')]
fmt-check *args:
    #!/usr/bin/bash
    set -euo pipefail
    cargo fmt --check
    if [[ "{{ args }}" == *"--verbose"* ]]; then taplo fmt --check {{ args }}; else RUST_LOG=warn taplo fmt --check; fi
    markdown_files=()
    while IFS= read -r -d '' file; do [[ -f "$file" ]] && markdown_files+=("$file"); done < <(git ls-files --cached --others --exclude-standard -z -- '*.md')
    if ((${#markdown_files[@]})); then rumdl fmt --check "${markdown_files[@]}"; fi
    cargo run --quiet -p xtask -- check-dioxus-format

# Run architecture policy, dependency checks, and workspace Clippy.
[group('quality')]
lint:
    cargo run --quiet -p xtask -- check-structure
    cargo check --locked -p gtl-parser --no-default-features --target wasm32-unknown-unknown
    cargo check --locked -p gtl-parser --all-features --all-targets --target wasm32-unknown-unknown
    cargo clippy --workspace --all-targets

# Complete read-only quality gate: formatting, lint, and configured ast-grep rules.
[group('quality')]
check: fmt-check lint
    ast-grep scan

# Apply Clippy fixes first, then normalize every formatter; extra args go to Clippy.
[group('quality')]
fix *args:
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged {{ args }}
    just fmt

# Rebuild web assets and fail if the tracked stylesheet drifts from its sources.
[group('quality')]
drift-check:
    cargo run --quiet -p xtask -- drift-check

# Report missing Mise-managed tools without changing the host.
[group('setup')]
doctor:
    @mise ls --local --missing --locked --no-header

# Cross-build all three Win11 exes; runs `just test --all` first unless -f/--force. --smoke selects a debug linkage build; use `--smoke --force` for the fast smoke path.
[group('windows')]
ship *args:
    cargo run --quiet -p xtask -- ship {{ args }}

# Configure this clone, build, and install git-tools.
[group('setup')]
setup:
    cargo run --quiet -p xtask -- setup

# Converge the Ubuntu development environment and run repository setup.
[group('setup')]
bootstrap *args:
    mise bootstrap --yes {{ args }}
