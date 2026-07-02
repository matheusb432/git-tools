# xtask

`xtask` is this repo's **embedded dev/release automation harness** (the cargo-xtask pattern). It is a workspace member built on demand and **never installed** — invoked only through this repo's own justfile as `cargo run -p xtask -- <verb>`.

## Verbs

| Verb | What it does | Justfile entry |
| -- | -- | -- |
| `bootstrap` | Full post-toolchain bring-up: link `.claude/skills` (via the PAL symlink primitive), build + install both artifacts, ensure `~/.local/bin` is on PATH. Migrates `install-git-tools.sh` + the old skills-link recipe. | `just bootstrap` |
| `install [--target cli\|viewer\|both]` | Place the prebuilt CLI (`git-tools` + `gtl` alias + `gtl-daemon`) and/or the viewer on PATH (idempotent byte-compare → installed/updated/unchanged; atomic replace is warm-tray-safe). Migrates `scripts/install.sh`. | `just install` / `just cli install` / `just desktop install` |
| `uninstall [--remove-config] [--force]` | Remove the installed binaries + alias + `gtl-daemon`; optionally delete repo-local config (guarded). | `just uninstall` |
| `test [--verbose] [--all]` | `cargo test` (terse); `--verbose` streams; `--all` adds the bun frontend tests. Migrates `just test`. | `just test` |
| `fmt [--check]` | Pinned-nightly `cargo fmt` (toolchain from `.rustfmt-nightly`) + taplo. `--check` additionally runs the `check-structure` and `check-deps` architecture lints. Migrates `just fmt` / `just fmt-check`. | `just fmt` / `just fmt-check` |
| `check-structure` | Mechanical layout lint over `crates/*/src` and `shared/*/src`: max dir depth 2, no nested `errors/`/`events/` dirs, no `services/` dir. Only wired into `fmt --check`; no standalone justfile entry. | (via `just fmt-check`) |
| `check-deps` | Dependency-direction lint over every `crates/*/Cargo.toml` and `shared/*/Cargo.toml` dependency table (`dependencies`, `dev-dependencies`, `build-dependencies`, and their `target.<cfg>` forms): `shared/*` never depends on an app crate, and the core crates (`domain`, `application`, `contracts`) never depend outward. Only wired into `fmt --check`; no standalone justfile entry. | (via `just fmt-check`) |
| `drift-check` | Rebuild the committed JS bundles and fail if they drift from their TS sources (CI/pre-commit gate). | `just drift-check` |
| `gen-icon` | Render the gtl-viewer icon assets — `icon.png` (1024²) + a multi-resolution `icon.ico` — from code with tiny-skia (the `.ico` is required by tauri-build on Windows). | `just desktop gen-icon` |
| `ship [--smoke]` | Cross-build the three Win11 release exes (CLI + gtl-daemon + viewer) from a Linux host via `cargo-xwin`, with a host-testable preflight; `--smoke` is a fast debug-profile linkage drift check (no artifact verify). | `just ship` / `just ship --smoke` |

## Why this is not a `new-rust-cli`

This is the embedded kind, not an installable tool. It deliberately ships **no install shim, no scoop manifest, no `install` recipe, and no global-shim runbook** — those are forbidden for embedded automation crates. If you need a tool on PATH / shared across repos, scaffold with `just repos new-rust-cli` instead.

## Layout

- `Cargo.toml` — lean workspace-member crate, `publish = false`, clap + anyhow + `which` + `gtl-platform` (+ `assert_cmd`/`predicates`/`tempfile` dev-deps).
- `src/main.rs` — thin entrypoint: parse argv, dispatch one verb, map any `Err` to a nonzero exit.
- `src/cli.rs` — the clap-derive `Subcommand` verb surface; doc comments are the `--help` SSOT.
- `src/proc.rs` — shared child-process `run`/`run_in` + the `RESULT scope=… status=…` contract helpers.
- `src/{bootstrap,install,testing,fmt,drift,icon,ship}.rs` — one verb per module: pure helpers (unit-tested) + thin glue.
- `tests/cli.rs` — `assert_cmd` arg-surface tests.
- `bootstrap.sh` — the one POSIX-shell seam: installs the Rust toolchain, then `exec`s `cargo run -p xtask -- bootstrap`.

## Wiring it into the host repo

See `justfile.snippet`:

1. Add `xtask` (or wherever it lives) to the host workspace's root `Cargo.toml` `members`.
2. Add the one-line forwarder recipes (`gen-icon`, `ship`, …) to the host justfile.

## Adding a verb

Add an arm to `cli::Command` (its doc comment is the `--help` text), a handler, and an arg-surface test. Express mutually-exclusive flags with clap's `conflicts_with`, not a runtime guard. Route captured runs through `src/proc.rs`; if the repo grows a `gate` binary, delegate to it rather than reimplementing capture.

## Build & test

```bash
cargo run -p xtask -- gen-icon     # run a verb (or: ship --smoke)
cargo test -p xtask                # arg-surface + unit tests
```
