set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

[private]
_install := 'scripts/install.sh'

_default:
    @just --list cli

# Build the release CLI engine + the offline diff-preview bundle: target/release/git-tools[.exe].
[group('cli')]
build: build-js
    cargo build --release

# Place the prebuilt git-tools binary + gtl alias on PATH (~/.local/bin). Build first with `just cli build`.
[group('cli')]
install:
    bash "{{ _install }}" install-cli

# Build + install the CLI engine only (git-tools + gtl). Leaves the desktop viewer untouched.
[group('cli')]
update: build install

# NODE_ENV=production forces lit's production build (no dev URLs in string literals).
# Bundle src/assets/ts -> src/assets/generated/preview.js (committed). Needs bun.
[group('cli')]
build-js:
    if command -v bun >/dev/null 2>&1; then \
      NODE_ENV=production bun build src/assets/ts/index.ts --outfile src/assets/generated/preview.js --format=iife --minify --target=browser; \
      echo "built src/assets/generated/preview.js"; \
    else echo "bun not installed; skipping build-js (commit generated/ unchanged)" >&2; fi

# --isolate gives each test file a fresh module registry so Lit's module-level init
# (which reads globalThis.document) can't be polluted by stubs from sibling tests.
# Run the TypeScript unit tests for src/assets/ts. Needs bun.
[group('cli')]
test-js:
    if command -v bun >/dev/null 2>&1; then bun test --isolate src/assets/ts; else echo "bun not installed; skipping test-js" >&2; fi
