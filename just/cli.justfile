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
# Bundle frontend/diff -> crates/infra/src/embedded/generated/preview.js (committed). Needs Deno.
[group('cli')]
build-js:
    NODE_ENV=production deno task --frozen build
    @echo "built crates/infra/src/embedded/generated/preview.js"

# Type-check with TS7 and run the frontend unit tests (diff preview + shared enhancers). Needs Deno.
[group('cli')]
test-js:
    deno task --frozen typecheck
    deno task --frozen test
