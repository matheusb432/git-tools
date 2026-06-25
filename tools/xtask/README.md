# xtask

`xtask` is this repo's **embedded dev/release automation harness** (the cargo-xtask pattern, ADR-0010). It is a workspace member built on demand and **never installed** — invoked only through this repo's own justfile as `cargo run -p xtask -- <verb>`.

## Why this is not a `new-rust-cli`

This is the embedded kind, not an installable tool. It deliberately ships **no install shim, no scoop manifest, no `install` recipe, and no global-shim runbook** — those are forbidden for embedded automation crates (ADR-0010). If you need a tool on PATH / shared across repos, scaffold with `just repos new-rust-cli` instead.

## Layout

- `Cargo.toml` — lean workspace-member crate, `publish = false`, clap + anyhow (+ `assert_cmd`/`predicates`/`tempfile` dev-deps).
- `src/main.rs` — thin entrypoint: parse argv, dispatch one verb, map any `Err` to a nonzero exit.
- `src/cli.rs` — the clap-derive `Subcommand` verb surface; doc comments are the `--help` SSOT.
- `src/proc.rs` — shared child-process `run`/`run_in` + the `RESULT scope=… status=…` contract helpers.
- `tests/cli.rs` — `assert_cmd` arg-surface tests.
- `bootstrap.sh` — the one POSIX-shell seam: installs the Rust toolchain, then `exec`s `cargo run -p xtask -- bootstrap`.

## Wiring it into the host repo

See `justfile.snippet`:

1. Add `tools/xtask` (or wherever it lives) to the host workspace's root `Cargo.toml` `members`.
2. Add the one-line forwarder recipes (`bootstrap`, `check`, …) to the host justfile.

## Adding a verb

Add an arm to `cli::Command` (its doc comment is the `--help` text), a handler, and an arg-surface test. Express mutually-exclusive flags with clap's `conflicts_with`, not a runtime guard. Route captured runs through `src/proc.rs`; if the repo grows a `gate` binary, delegate to it rather than reimplementing capture.

## Build & test

```bash
cargo run -p xtask -- check     # run a verb
cargo test -p xtask             # arg-surface + unit tests
```
