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

# Build + install everything: the CLI engine (git-tools + gtl) and the desktop viewer (gtl-viewer).
[group('build')]
update:
    just cli update
    just desktop update

# Remove the installed binary/alias + viewer. Add --remove-config to also delete git-tools.toml/secrets (confirms).
[group('build')]
uninstall *args:
    bash "scripts/install.sh" uninstall {{ args }}

# Build only if the binary is missing (preflight for run recipes).
_preflight:
    test -x "{{ _bin }}" || cargo build --release

# ============ quality ============

# cargo tests + the install ShellSpec suite.
[group('quality')]
test: _preflight _require-shellspec
    cargo test
    shellspec

# Fail loud with a fix hint when ShellSpec is not on PATH.
_require-shellspec:
    command -v shellspec >/dev/null 2>&1 || { echo "shellspec not found — install per AGENTS.md" >&2; exit 1; }

# Format all TOML with taplo (no-op if taplo is absent).
[group('quality')]
fmt:
    if command -v taplo >/dev/null 2>&1; then taplo fmt; else echo "taplo not installed; skipping"; fi

# Check TOML formatting without writing.
[group('quality')]
fmt-check:
    if command -v taplo >/dev/null 2>&1; then taplo fmt --check; else echo "taplo not installed; skipping"; fi

# Fail if the committed bundles drift from their TS sources (preview bundle + viewer shell).
_js-drift-guard:
    if command -v bun >/dev/null 2>&1; then \
      just cli build-js >/dev/null; \
      just desktop build-viewer-ui >/dev/null; \
      git diff --exit-code -- src/assets/generated/ || { echo "generated/ is stale — run 'just cli build-js' and commit" >&2; exit 1; }; \
      git diff --exit-code -- crates/desktop/dist/ || { echo "crates/desktop/dist/ is stale — run 'just desktop build-viewer-ui' and commit" >&2; exit 1; }; \
    else echo "bun absent; skipping js drift guard" >&2; fi

# One-time repo setup: link .claude/skills -> .agents/skills so Claude Code sees cross-agent skills.
bootstrap:
    mkdir -p .claude
    ln -sfn ../.agents/skills .claude/skills
    @echo "linked .claude/skills -> .agents/skills"
