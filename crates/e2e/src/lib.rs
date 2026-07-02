//! `crates/e2e` is a test-only crate: it has no production code of its own.
//!
//! Its `tests/roundtrip.rs` is the walking-skeleton test proving the full
//! stack wires together — the `git-tools` CLI autostarts the resident
//! `gtl-daemon`, renders a diff through it into the content-addressed store,
//! reuses the warm daemon on a second run, and the daemon stops cleanly on
//! `git-tools daemon stop`. See `crates/cli/tests/e2e.rs` for the much larger
//! per-command behavior suite this crate does not duplicate.
