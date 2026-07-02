set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list cli

# Build the release CLI engine + the offline diff-preview bundle: target/release/git-tools[.exe].
[group('cli')]
build: build-js
    cargo build --release -p cli

# Place the prebuilt git-tools binary + gtl alias on PATH (~/.local/bin). Build first with `just cli build`.
[group('cli')]
install:
    cargo run --quiet -p xtask -- install --target cli

# Build + install the CLI engine only (git-tools + gtl). Leaves the desktop viewer untouched.
[group('cli')]
update: build install

# NODE_ENV=production forces frontend dependencies' production builds.
# Bundle frontend/diff -> crates/cli/src/embedded/generated/preview.js (committed). Needs bun.
[group('cli')]
build-js:
    if command -v bun >/dev/null 2>&1; then \
      NODE_ENV=production bunx vite build --config frontend/diff/vite.config.mjs; \
      echo "built crates/cli/src/embedded/generated/preview.js"; \
    else echo "bun not installed; skipping build-js (commit embedded/generated unchanged)" >&2; fi

# Run the TypeScript unit tests for the diff and viewer frontends. Needs bun.
[group('cli')]
test-js:
    if command -v bun >/dev/null 2>&1; then bun test --isolate frontend/diff frontend/viewer; else echo "bun not installed; skipping test-js" >&2; fi
