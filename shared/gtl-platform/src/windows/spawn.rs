//! Windows effectful spawn applier. The OS-bound primitive is `creation_flags`
//! (windows `CommandExt`); the *decision* (`DetachStrategy::Windows`) lives in
//! `policy.rs`. The Windows analogue of Linux's `process_group(0)`.
use std::{
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

/// `DETACHED_PROCESS` (Win32 process-creation flag): the child is created without
/// a console and does not inherit or attach to the launcher's console.
const DETACHED_PROCESS: u32 = 0x0000_0008;
/// `CREATE_NO_WINDOW` (Win32): the child runs with no console window; paired with
/// `DETACHED_PROCESS` so launching the GUI viewer never flashes a console.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Spawn `program args…` detached: no inherited console, null stdio, never waited
/// on. Fire-and-forget — the Windows analogue of the Linux `process_group(0)`.
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
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }
    command.spawn().map(|_child| ())
}
