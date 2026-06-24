//! Linux effectful spawn applier. The only OS-bound primitive here is
//! `process_group(0)` (unix `CommandExt`); the *decision* lives in `policy.rs`.
use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

/// Spawn `program args…` detached: own process group so the CLI's Ctrl-C does
/// not reach it, null stdio, and we never wait. Fire-and-forget.
pub fn spawn_detached(program: &Path, args: &[&str]) -> std::io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.process_group(0);
    cmd.spawn().map(|_child| ())
}
