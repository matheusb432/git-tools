//! `gtl daemon status|restart|stop`: bounded manual control of the resident daemon.

use crate::client;

/// Report whether the daemon is running.
///
/// # Errors
///
/// Returns lifecycle errors from the daemon client, including an unhealthy owner.
pub fn status() -> anyhow::Result<()> {
    match client::daemon_status()? {
        client::DaemonStatusOutcome::Running(status) => println!(
            "gtl-daemon running (pid {}, port {}, version {})",
            status.pid, status.port, status.version
        ),
        client::DaemonStatusOutcome::NotRunning => println!("gtl-daemon not running"),
        client::DaemonStatusOutcome::OwnedUnhealthy => {
            anyhow::bail!("daemon lock is owned but no healthy daemon responded");
        }
    }
    Ok(())
}

/// Ask a running daemon to exit.
///
/// # Errors
///
/// Returns lifecycle errors from the daemon client.
pub fn stop() -> anyhow::Result<()> {
    match client::daemon_stop()? {
        client::DaemonStopOutcome::Stopped => println!("gtl-daemon stopped"),
        client::DaemonStopOutcome::NotRunning => println!("gtl-daemon not running"),
    }
    Ok(())
}

/// Replace a healthy daemon, or start one when none is running.
///
/// # Errors
///
/// Returns lifecycle errors from the daemon client.
pub fn restart() -> anyhow::Result<()> {
    let status = client::daemon_restart()?;
    println!(
        "gtl-daemon restarted (pid {}, port {}, version {})",
        status.pid, status.port, status.version
    );
    Ok(())
}
