use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

pub(super) fn spawn(program: &Path, args: &[&str]) -> std::io::Result<()> {
    spawn_command(program, args, None)
}

pub(super) fn spawn_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> std::io::Result<()> {
    spawn_command(program, arguments, Some(working_directory))
}

fn spawn_command(
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{
        fs,
        path::Path,
        thread,
        time::{Duration, Instant},
    };

    use super::spawn_in;

    const DETACHED_CHILD_MARKER: &str = "detached-child.marker";
    const DETACHED_CHILD_PID: &str = "detached-child.pid";

    #[test]
    fn detached_short_lived_child_is_not_left_as_a_zombie() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::write(temporary.path().join(DETACHED_CHILD_MARKER), b"ready")
            .expect("write child marker");
        let current_executable = std::env::current_exe().expect("current test executable");

        spawn_in(
            &current_executable,
            &[
                "detached_child_records_pid_and_exits",
                "--ignored",
                "--nocapture",
            ],
            temporary.path(),
        )
        .expect("spawn detached child");

        let process_id = wait_for_process_id(
            &temporary.path().join(DETACHED_CHILD_PID),
            Duration::from_secs(1),
        );
        let process_path = std::path::PathBuf::from(format!("/proc/{process_id}"));
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if !process_path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let state = fs::read_to_string(process_path.join("stat")).unwrap_or_default();
        panic!("detached process {process_id} was not reaped: {state}");
    }

    #[test]
    #[ignore = "subprocess helper"]
    fn detached_child_records_pid_and_exits() {
        if !Path::new(DETACHED_CHILD_MARKER).is_file() {
            return;
        }
        fs::write(DETACHED_CHILD_PID, std::process::id().to_string())
            .expect("write detached child pid");
    }

    fn wait_for_process_id(path: &Path, timeout: Duration) -> u32 {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(value) = fs::read_to_string(path) {
                return value.trim().parse().expect("numeric process id");
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {}", path.display());
    }
}
