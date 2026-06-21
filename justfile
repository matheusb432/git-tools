set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]

_binname := if os() == "windows" { "git-tools.exe" } else { "git-tools" }
_bin := justfile_directory() / "target" / "release" / _binname
_install := justfile_directory() / "scripts" / "install.sh"

_default:
    @just --list --unsorted

# Print the git-tools command reference.
help: _preflight
    @"{{_bin}}" --help

# ============ build / install ============

# Build the release binary at target/release/git-tools[.exe].
[group('build')]
build:
    cargo build --release

# Build only if the binary is missing (preflight for run recipes).
_preflight:
    test -x "{{_bin}}" || cargo build --release

# Install or refresh the binary + gtl alias on PATH (Linux; Windows via Git Bash).
[group('build')]
install:
    bash "{{_install}}" install

# Remove the installed binary/alias. Add --remove-config to also delete git-tools.toml/secrets (confirms).
[group('build')]
uninstall *args:
    bash "{{_install}}" uninstall {{args}}

# Rebuild and refresh the installed binary in place.
[group('build')]
update:
    bash "{{_install}}" install

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

# Bundle src/assets/ts -> src/assets/generated/preview.js (committed). Needs bun.
# NODE_ENV=production forces lit's production build (no dev URLs in string literals).
[group('build')]
build-js:
    if command -v bun >/dev/null 2>&1; then \
      NODE_ENV=production bun build src/assets/ts/index.ts --outfile src/assets/generated/preview.js --format=iife --minify --target=browser; \
      echo "built src/assets/generated/preview.js"; \
    else echo "bun not installed; skipping build-js (commit generated/ unchanged)" >&2; fi

# Run the TypeScript unit tests. Needs bun.
# --isolate gives each test file a fresh module registry so Lit's module-level init
# (which reads globalThis.document) can't be polluted by stubs from sibling tests.
[group('quality')]
test-js:
    if command -v bun >/dev/null 2>&1; then bun test --isolate src/assets/ts; else echo "bun not installed; skipping test-js" >&2; fi

# Fail if the committed bundle drifts from its TS source.
_js-drift-guard:
    if command -v bun >/dev/null 2>&1; then \
      just build-js >/dev/null; \
      git diff --exit-code -- src/assets/generated/ || { echo "generated/ is stale — run 'just build-js' and commit" >&2; exit 1; }; \
    else echo "bun absent; skipping js drift guard" >&2; fi

# One-time repo setup: link .claude/skills -> .agents/skills so Claude Code sees cross-agent skills.
bootstrap:
    mkdir -p .claude
    ln -sfn ../.agents/skills .claude/skills
    @echo "linked .claude/skills -> .agents/skills"
