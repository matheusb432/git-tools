use std::{
    os::unix::process::CommandExt as _,
    path::Path,
    process::{Command, Stdio},
};

pub(super) fn spawn(program: &Path, focus_window: bool) -> std::io::Result<()> {
    let mut command = Command::new(program);
    command
        .arg(if focus_window {
            "--focus-diff"
        } else {
            "--background-diff"
        })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: this runs after fork and only calls async-signal-safe libc functions.
    // The intermediate child exits so the long-running viewer is not owned by the server.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            let process_id = libc::fork();
            if process_id == -1 {
                return Err(std::io::Error::last_os_error());
            }
            if process_id > 0 {
                libc::_exit(0);
            }
            Ok(())
        });
    }
    let mut intermediate = command.spawn()?;
    intermediate.wait().map(|_| ())
}
