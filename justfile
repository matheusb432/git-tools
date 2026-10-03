set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set positional-arguments

mod cli 'just/cli.justfile'
mod desktop 'just/desktop.justfile'
mod server 'just/server.justfile'
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

# Serve the development-only component story catalog in a browser.
[group('build')]
preview-components *args:
    cargo run --quiet -p xtask -- component-preview-serve -- {{ args }}

# Build both the CLI engine and the desktop viewer.
[group('build')]
build:
    cargo run --quiet -p xtask -- build

# Place both the prebuilt CLI engine and desktop viewer on PATH. Build first with `just build`.
[group('build')]
install:
    cargo run --quiet -p xtask -- install --target both

# Build + install everything: the CLI engine (git-tools + gtl + gtl-server) and the desktop viewer (gtl-viewer).
[group('build')]
update:
    just build
    just install

# Remove the installed CLI, server, alias, viewer, desktop entry, and icon; preserve configuration.
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
    test -x "{{ _bin }}" || cargo build --release -p gtl-cli -p gtl-server

# Compare Criterion benchmarks against the local baseline. Use --update to replace it.
[arg("benchmark", help="Benchmark target or all", pattern="all|app-state-mutations|app-state-record-render|grpc-requests|parser-syntax|story-set-registry|user-settings|view-cache|viewer-render|web-rows")]
[arg("case", help="Exact Criterion benchmark case")]
[arg("update", long="update", value="--save-baseline local", help="Compare and replace the local baseline")]
[arg("quick", long="quick", value="--quick", help="Stop once Criterion reaches statistical significance")]
[group('performance')]
bench benchmark="all" case="" update="--baseline local" quick="":
    CRITERION_HOME="{{ justfile_directory() }}/.artifacts/benchmarks/criterion" cargo bench --locked -p gtl-benchmarks {{ if benchmark == "all" { "--benches" } else { "--bench " + replace(benchmark, "-", "_") } }} -- {{ if case == "" { "" } else { quote(case) + " --exact" } }} {{ update }} {{ quick }}

# Regenerate and verify the immutable desktop viewer scroll fixture under bounded resources.
[group('performance')]
bench-scroll-fixture-update:
    cargo run --quiet -p xtask -- desktop-scroll-fixture

# Compare production desktop scrolling against the local baseline. Use --update to replace it.
[arg("update", long="update", value="--update", help="Compare and replace the local baseline")]
[group('performance')]
[arg("quick", long="quick", value="--quick", help="Use five interaction samples per launch")]
bench-scroll update="" quick="":
    cargo run --quiet -p xtask -- desktop-scroll-benchmark {{ update }} {{ quick }}

# Compare release terminal clicks, wheel bursts, and refreshes against a local baseline.
[arg("update", long="update", value="--update", help="Compare and replace the local baseline")]
[group('performance')]
[unix]
bench-tui update="":
    GTL_TUI_BENCH_UPDATE="{{ update }}" cargo test --locked --release -p gtl-cli --test diff_tui_benchmark -- --ignored --nocapture

# Compare release server highlighting against the local baseline. Use --update to replace it.
[arg("update", long="update", value="--update", help="Compare and replace the local baseline")]
[group('performance')]
bench-highlight update="":
    cargo run --quiet -p xtask -- server-highlighting-benchmark {{ update }}

# Compare demand-loaded viewer source construction against the local baseline.
[arg("update", long="update", value="--update", help="Compare and replace the local baseline")]
[group('performance')]
bench-view-source update="":
    cargo run --quiet -p xtask -- view-source-benchmark {{ update }}

# Compare the release gRPC transport against the local ghz baseline. Use --update to replace it.
[arg("update", long="update", value="--update", help="Compare and replace the local baseline")]
[group('performance')]
[linux]
bench-grpc update="":
    @just _bench-grpc 30m {{ update }}

# Validate the release gRPC transport with a short workload that never touches the baseline.
[group('performance')]
[linux]
bench-grpc-smoke:
    @just _bench-grpc 2m --smoke

[private]
[linux]
_bench-grpc timeout *args:
    cargo build --locked --release --jobs 1 -p gtl-server --bin gtl-server
    cargo run --locked --release --jobs 1 -p gtl-benchmarks --bin grpc-transport-driver \
        --config 'target."cfg(all())".runner = ["systemd-run", "--user", "--scope", "--quiet", "--collect", "--property=CPUQuota=200%", "--property=MemoryMax=2147483648", "--property=MemorySwapMax=0", "--property=TasksMax=128", "/usr/bin/nice", "-n", "10", "/usr/bin/timeout", "--signal=TERM", "--kill-after=15s", "{{ timeout }}"]' \
        -- "${@:2}"

# Attribute full-language release server highlighting allocations with Valgrind Massif.
[group('performance')]
profile-highlight:
    cargo run --quiet -p xtask -- server-highlighting-profile

# Run default-member tests with Nextest. With no arguments, also run the parser and web feature matrix.
[group('quality')]
test *args:
    @cargo nextest run "$@"
    @if [ "$#" -eq 0 ]; then just _test-default-matrix; fi

[private]
_test-default-matrix:
    @just test-parser
    @just test-web-desktop
    @just test-web-component-preview

# Run the parser test suite with every feature enabled.
[group('quality')]
test-parser *args:
    @cargo nextest run --locked -p gtl-parser --all-features "$@"

# Run the native desktop configuration of the shared web crate.
[group('quality')]
test-web-desktop *args:
    @cargo nextest run --locked -p gtl-web "$@"

# Run the CSR component-preview configuration and its story registry tests.
[group('quality')]
test-web-component-preview *args:
    @cargo nextest run --locked -p gtl-web --no-default-features --features component-preview "$@"

# Run workspace doctests, which Nextest does not execute.
[group('quality')]
test-docs:
    @cargo test --locked --doc --workspace

# Run the release-built desktop journeys in parallel. Arguments select journeys through Nextest, such as `just test-e2e live_diff`.
[group('quality')]
test-e2e *args:
    @timeout --signal=TERM --kill-after=30s 2h cargo run --quiet -p xtask -- desktop-e2e-worker journeys "$@"

# Run the selected desktop journeys and retain passing screenshots in addition to failure evidence.
[group('quality')]
test-e2e-evidences *args:
    @TEST_EVIDENCES_OUTPUT_PATH="{{ justfile_directory() }}/.artifacts/e2e" just test-e2e "$@"

# Check native window focus when the CLI opens diffs into the release viewer.
[group('quality')]
test-e2e-native:
    @timeout --signal=TERM --kill-after=30s 2h cargo run --quiet -p xtask -- desktop-e2e-worker native

# Run the Playwright offline-artifact and component-preview journeys.
[group('quality')]
test-e2e-browser:
    @timeout --signal=TERM --kill-after=30s 2h cargo run --quiet -p xtask -- desktop-e2e-worker browser

# Run every Rust target, feature supplement, doctest, and viewer journey.
[group('quality')]
test-all:
    @cargo nextest run --workspace
    @just test-parser
    @just test-web-component-preview
    @just test-docs
    @just test-e2e
    @just test-e2e-native
    @just test-e2e-browser

# Collect Nextest and doctest coverage, then remove the isolated target directory.
[group('quality')]
coverage *args:
    @cargo llvm-cov nextest "$@"
    @cargo +nightly llvm-cov --doc --workspace
    @cargo clean --target-dir target/llvm-cov-target

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
    buf lint
    cargo run --quiet -p xtask -- check-structure
    cargo check --locked -p gtl-application --all-targets --all-features
    cargo check --locked -p gtl-parser --no-default-features --target wasm32-unknown-unknown
    cargo run --quiet -p xtask -- check-parser-wasm
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Complete read-only quality gate: formatting, lint, and configured ast-grep rules.
[group('quality')]
check: fmt-check lint
    ast-grep scan

# Apply Clippy fixes first, then normalize every formatter; extra args go to Clippy.
[group('quality')]
fix *args:
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged {{ args }}
    just fmt

# Report missing Mise-managed tools without changing the host.
[group('setup')]
doctor:
    @mise ls --local --missing --locked --no-header

# Compile optimized native Tree-sitter dependencies into Cargo's reusable debug cache.
[group('setup')]
prepare-tree-sitter:
    cargo build --locked -p gtl-parser --features syntax

# Cross-build all three Win11 exes; runs `just test-all` first unless -f/--force. --smoke selects a debug linkage build; use `--smoke --force` for the fast smoke path.
[group('windows')]
[linux]
ship *args:
    cargo run --locked --quiet -p xtask -- ship {{ args }}

# Lint Windows-specific application code after staging the viewer with `just web build`.
[group('windows')]
[linux]
check-windows:
    TAURI_CONFIG="$(cat crates/gtl-desktop/tauri.production.conf.json)" \
        cargo xwin clippy --locked --target x86_64-pc-windows-msvc --all-targets \
        -p gtl-cli -p gtl-server -p gtl-desktop -p gtl-infra -p gtl-client \
        -p gtl-local-transport -p gtl-application -p gtl-models -p gtl-artifacts \
        -p gtl-parser -p gtl-wire --features gtl-desktop/custom-protocol -- -D warnings

# Archive the prebuilt Windows tests for execution on a Windows host.
[group('windows')]
[linux]
archive-windows-tests:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p .artifacts/windows
    eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
    unset CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER
    cargo nextest archive --locked --release --target x86_64-pc-windows-msvc \
        -p gtl-cli -p gtl-server -p gtl-infra -p gtl-client -p gtl-local-transport \
        -p gtl-application -p gtl-models -p gtl-artifacts -p gtl-parser -p gtl-wire \
        --archive-file .artifacts/windows/tests.tar.zst

# Package Linux or Windows installers, verification archives, and checksums.
[group('release')]
[arg('platform', pattern='linux|windows')]
[linux]
package platform:
    cargo run --locked --quiet -p xtask -- release-package {{ platform }}

# Configure this clone, build, and install git-tools.
[group('setup')]
setup:
    just update
    cargo run --quiet -p xtask -- ensure-install-path

# Converge the Ubuntu development environment and run repository setup.
[group('setup')]
bootstrap *args:
    mise bootstrap --yes {{ args }}
