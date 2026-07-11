set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '..'

_default:
    @just --list cli

# Build the release CLI engine + gtl-daemon + the offline diff-preview bundle: target/release/{git-tools,gtl-daemon}[.exe].
[group('cli')]
build: build-js
    cargo build --release -p cli -p daemon

# Place the prebuilt git-tools binary + gtl alias + gtl-daemon on PATH (~/.local/bin). Build first with `just cli build`.
[group('cli')]
install:
    cargo run --quiet -p xtask -- install --target cli

# Build + install the CLI engine only (git-tools + gtl + gtl-daemon). Leaves the desktop viewer untouched.
[group('cli')]
update: build install

# NODE_ENV=production forces frontend dependencies' production builds.
# Bundle frontend/diff -> crates/infra/src/embedded/generated/preview.js (committed). Needs bun.
[group('cli')]
build-js:
    if command -v bun >/dev/null 2>&1; then \
      NODE_ENV=production bunx vite build --config frontend/diff/vite.config.mjs; \
      echo "built crates/infra/src/embedded/generated/preview.js"; \
    else echo "bun not installed; skipping build-js (commit embedded/generated unchanged)" >&2; fi

# Run the TypeScript unit tests and mounted viewer component tests. Needs bun.
[group('cli')]
test-js:
    if command -v bun >/dev/null 2>&1; then \
      bun test --isolate --path-ignore-patterns='**/*.{component,query}.test.ts' frontend/diff frontend/viewer; \
      bun run test:viewer-components; \
      bun run check:viewer-fsd; \
    else echo "bun not installed; skipping test-js" >&2; fi
