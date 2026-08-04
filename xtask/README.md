# xtask

`xtask` is this repo's **embedded dev/release automation harness** (the cargo-xtask pattern). It is a workspace member built on demand and **never installed** — invoked only through this repo's own justfile as `cargo run -p xtask -- <verb>`.

## Verbs

| Verb | What it does | Justfile entry |
| -- | -- | -- |
| `bootstrap` | Repository-local bootstrap phase: configure the tracked `.githooks` directory, install frontend dependencies, build and install both artifacts, and ensure `~/.local/bin` is on PATH. Mise runs it after converging the host environment. | `just bootstrap` |
| `install [--target cli\|viewer\|both]` | Place the prebuilt CLI (`git-tools` + `gtl` alias + `gtl-daemon`) and/or the viewer on PATH (idempotent byte-compare → installed/updated/unchanged; atomic replace is warm-tray-safe). Migrates `scripts/install.sh`. | `just install` / `just cli install` / `just desktop install` |
| `uninstall [--remove-config] [--force]` | Remove the CLI, alias, daemon, viewer, desktop entry, and icon; optionally delete repo-local config. | `just uninstall` / `just purge` |
| `test [--verbose] [--scope unit\|e2e\|all]` | Default (`unit`): default-member Rust tests. `--e2e`: hermetic native and browser E2E. `--all`: all Rust, frontend, drift, and E2E. Run `just check` separately for the read-only quality gate. `--e2e`/`--all` are shorthands for `--scope`. | `just test` |
| `test coverage [cargo-llvm-cov args...]` | Collect workspace coverage with `cargo llvm-cov`; extra arguments are forwarded. | `just test coverage` |
| `fmt` | Pinned-nightly rustfmt, Taplo, rumdl, and Oxfmt, in place. | `just fmt` |
| `fmt-check` | Verify formatting without writing (formatting only; exits non-zero on drift). | `just fmt-check` |
| `lint` | Oxlint, the architecture lints, and full-workspace Clippy including desktop. | `just lint` |
| `check` | Complete read-only gate: formatting drift, then every linter. | `just check` |
| `pre-commit` | Check staged whitespace and run only the formatters or frontend linter owned by staged files. | tracked Git hook |
| `fix [clippy args...]` | Apply Clippy and Oxlint fixes before normalizing every formatter. | `just fix` |
| `build [--target cli\|viewer\|both]` | Build mandatory release artifact sets; the root build never soft-skips the viewer. | `just build` / scoped build recipes |
| `frontend-test` | Type-check and unit-test the framework-free frontend. | `just cli test` |
| `bench` | Run the shared Rust benchmarks; `--fast` selects the concise viewer-render preset. | `just bench` |
| `check-structure` | Cargo dependency-direction policy for core, shared, adapter, and black-box test packages. Wired into `lint` / `check`; also runnable standalone. | (via `just check`) |
| `drift-check` | Rebuild the committed diff-preview JS bundle and fail if it drifts from its TypeScript sources. | `just drift-check` |
| `gen-icon` | Render the gtl-viewer icon assets — `icon.png` (1024²) + a multi-resolution `icon.ico` — from code with tiny-skia (the `.ico` is required by tauri-build on Windows). | `just desktop gen-icon` |
| `ship [--smoke] [--force]` | Run `just test --all` unless forced, then cross-build the three Win11 exes. `--smoke --force` is the fast linkage-only path. | `just ship` |

## Why this is not a `new-rust-cli`

This is the embedded kind, not an installable tool. It deliberately ships **no install shim, no scoop manifest, no `install` recipe, and no global-shim runbook** — those are forbidden for embedded automation crates. If you need a tool on PATH / shared across repos, scaffold with `just repos new-rust-cli` instead.

## Layout

- `Cargo.toml` — workspace-member dependency manifest; the desktop harness uses `command-group` for process-tree ownership and `zbus` for a private StatusNotifier watcher.
- `src/main.rs` — thin facade: parse argv, dispatch one verb, map any `Err` to a nonzero exit.
- `src/cli.rs` — the clap-derive `Subcommand` verb surface; doc comments are the `--help` SSOT and each arm's name comes from a `Verb` constant.
- `src/verb.rs` — the `Verb` name constants shared by the clap surface and the `RESULT` scopes.
- `src/process.rs` - shared child-process execution and the non-test `RESULT` contract helpers.
- `src/gate.rs` — captures a bash command's combined output into `.artifacts/logs/<scope>.log`, printing a terse PASS/FAIL line plus the `RESULT` contract line and tailing the log on failure.
- `src/task.rs` — labeled command steps (`Step`) plus `run_all` / `check_all` plan orchestration.
- `src/verbs/` — one module per verb, each owning its flags and workflow with pure helpers where command planning needs unit coverage; `format/` is the formatter matrix and linter gate, `install/` adds the Linux desktop-entry/icon submodule, and `check_structure.rs` is the Cargo architecture policy.
- `tests/architecture_policy.rs` — dependency-direction policy tests.
- `bootstrap.sh` — the one POSIX-shell seam: installs the pinned mise release, trusts the project configuration, then hands off to `mise bootstrap`.

## Adding a verb

Add an arm to `cli::Command` (its doc comment is the `--help` text) named via a `Verb` constant, a module under `src/verbs/`, and an arg-surface test. Express mutually-exclusive flags with clap's `conflicts_with`, not a runtime guard. Route child processes through `src/process.rs` (command plans through `src/task.rs`); captured runs delegate to `src/gate.rs` rather than reimplementing capture.

## Build & test

```bash
cargo run -p xtask -- gen-icon     # run a verb (or: ship --smoke)
cargo test -p xtask                # arg-surface + unit tests
```

## Browser E2E

Thirtyfour owns the release Tauri/WebKit one-shot and saved-live-view lifecycles. Playwright-Rust owns the raw-artifact lifecycle for a production-generated `file://` diff in Chromium. The typed xtask installs browser assets, isolates each spec's mutable state, runs the suites serially, reaps their process trees, and writes logs and `report.json`.

```bash
just test --e2e
just test --e2e --evidences
just test --all
just test --all --evidences
```

`--evidences` writes one success PNG for each of the three browser workflows under `.artifacts/e2e/success`. Failing workflows always write their PNG under `.artifacts/e2e/fail`.
