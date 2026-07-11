//! `gtl diff live`: placeholder execution pending the htmx-based viewer surface.
//!
//! `diff live` was born native, forwarding saved live views to the `gtl-viewer` app.
//! That app surface no longer exists, so execution is stubbed out until a later plan
//! revives it. The clap variant, [`crate::cli::LiveArgs`], and `--help` text are
//! unchanged — only what running the command does.

/// Pending-htmx placeholder for `diff live`. Does not touch the daemon, persist
/// anything, or launch a viewer.
///
/// # Errors
/// Never returns an error.
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the fallible signature `diff_live_exit` (crates/cli/src/lib.rs) already dispatches on, so the stub can regrow real error paths without a call-site change"
)]
pub fn run(_path: Option<String>) -> anyhow::Result<()> {
    println!("TODO: pending htmx integration");
    Ok(())
}
