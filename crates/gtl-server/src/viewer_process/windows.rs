use std::{
    os::windows::process::CommandExt as _,
    path::Path,
    process::{Command, Stdio},
};

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(super) fn spawn(program: &Path) -> std::io::Result<()> {
    Command::new(program)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map(|_child| ())
}
