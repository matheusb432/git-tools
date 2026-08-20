use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

pub(super) fn spawn(program: &Path, arguments: &[&str]) -> std::io::Result<()> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the closure runs after fork and calls only async-signal-safe libc
    // functions. The grandchild returns to Command's exec path; the intermediate
    // exits immediately so this process can reap it.
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
