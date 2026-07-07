//! `gtl daemon status|stop`: manual control of the resident daemon. Both verbs
//! exit 0 whether or not a daemon is running; probe failures read as "not
//! running" rather than escalating.

use crate::client;

/// Report whether the daemon is running.
pub fn status() {
    match client::daemon_status() {
        Some(status) => println!(
            "gtl-daemon running (pid {}, port {}, version {})",
            status.pid, status.port, status.version
        ),
        None => println!("gtl-daemon not running"),
    }
}

/// Ask a running daemon to exit.
pub fn stop() {
    if client::daemon_stop() {
        println!("gtl-daemon stopped");
    } else {
        println!("gtl-daemon not running");
    }
}
