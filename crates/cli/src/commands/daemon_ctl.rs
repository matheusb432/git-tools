//! `gtl daemon status|stop`: manual control of the resident daemon. Both verbs
//! exit 0 whether or not a daemon is running; only a transport failure escalates
//! to the internal-error exit path.

use crate::client;

/// Report whether the daemon is running.
///
/// # Errors
/// Propagates a transport failure from the status probe.
pub fn status() -> anyhow::Result<()> {
    match client::daemon_status()? {
        Some(status) => println!(
            "gtl-daemon running (pid {}, port {}, version {})",
            status.pid, status.port, status.version
        ),
        None => println!("gtl-daemon not running"),
    }
    Ok(())
}

/// Ask a running daemon to exit.
///
/// # Errors
/// Propagates a transport failure from the stop request.
pub fn stop() -> anyhow::Result<()> {
    if client::daemon_stop()? {
        println!("gtl-daemon stopped");
    } else {
        println!("gtl-daemon not running");
    }
    Ok(())
}
