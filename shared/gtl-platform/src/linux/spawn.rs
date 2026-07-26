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
    spawn(program, args, None)
}

pub fn spawn_detached_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> std::io::Result<()> {
    spawn(program, arguments, Some(working_directory))
}

fn spawn(
    program: &Path,
    arguments: &[&str],
    working_directory: Option<&Path>,
) -> std::io::Result<()> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }
    command.process_group(0);
    command.spawn().map(|_child| ())
}
