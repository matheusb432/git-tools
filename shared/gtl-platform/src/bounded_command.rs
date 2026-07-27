use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    sync::mpsc::{TryRecvError, sync_channel},
    thread,
    time::{Duration, Instant},
};

use command_group::CommandGroup;

const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(10);
const COMMAND_CLEANUP_RESERVE_MAX: Duration = Duration::from_millis(250);

/// Runs a command in an owned process tree and collects bounded stdout.
///
/// # Examples
///
/// ```no_run
/// use std::{path::Path, time::Duration};
///
/// let output = gtl_platform::run_command_with_bounded_stdout_in(
///     Path::new("git"),
///     &["--version"],
///     Path::new("."),
///     Duration::from_secs(3),
///     16 * 1024,
/// )?;
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error when the command cannot start, exceeds the time or stdout
/// bounds, or its process tree cannot be terminated.
pub fn run_command_with_bounded_stdout_in(
    program: &Path,
    arguments: &[&str],
    working_directory: &Path,
    timeout: Duration,
    stdout_bytes_max: usize,
) -> io::Result<Output> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped());
    let mut child = command.group_spawn()?;
    let stdout = child
        .inner()
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("command did not provide stdout"))?;
    let (reader_sender, reader_receiver) = sync_channel(1);
    let reader = thread::spawn(move || {
        let result = read_bytes_bounded(stdout, stdout_bytes_max);
        let _ = reader_sender.send(result);
    });

    let started_at = Instant::now();
    let completion_deadline = started_at + timeout;
    let cleanup_reserve = (timeout / 4).min(COMMAND_CLEANUP_RESERVE_MAX);
    let execution_deadline = completion_deadline
        .checked_sub(cleanup_reserve)
        .unwrap_or(started_at);
    let mut exit_status = None;
    let mut stdout_result = None;
    let mut process_tree_termination_requested = false;
    let mut execution_timed_out = false;

    loop {
        if stdout_result.is_none() {
            match reader_receiver.try_recv() {
                Ok(result) => stdout_result = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    stdout_result = Some(Err(io::Error::other(
                        "command stdout reader stopped without a result",
                    )));
                }
            }
        }
        if exit_status.is_none() {
            exit_status = child.inner().try_wait()?;
        }

        let reader_failed = stdout_result
            .as_ref()
            .is_some_and(std::result::Result::is_err);
        let direct_child_exited_with_open_stdout = exit_status.is_some() && stdout_result.is_none();
        let execution_deadline_reached =
            exit_status.is_none() && Instant::now() >= execution_deadline;
        if !process_tree_termination_requested
            && (reader_failed || direct_child_exited_with_open_stdout || execution_deadline_reached)
        {
            terminate_process_tree(&mut child)?;
            process_tree_termination_requested = true;
            execution_timed_out = execution_deadline_reached;
        }

        if let Some(status) = exit_status
            && let Some(stdout) = stdout_result.take()
        {
            reader
                .join()
                .map_err(|_| io::Error::other("command stdout reader thread panicked"))?;
            if execution_timed_out {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("command exceeded {} ms", timeout.as_millis()),
                ));
            }
            return stdout.map(|stdout| Output {
                status,
                stdout,
                stderr: Vec::new(),
            });
        }

        let now = Instant::now();
        if now >= completion_deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "command process tree or stdout reader exceeded {} ms",
                    timeout.as_millis()
                ),
            ));
        }
        thread::sleep(COMMAND_POLL_INTERVAL.min(completion_deadline - now));
    }
}

fn terminate_process_tree(child: &mut command_group::GroupChild) -> io::Result<()> {
    match child.kill() {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn read_bytes_bounded(mut reader: impl Read, bytes_max: usize) -> io::Result<Vec<u8>> {
    const READ_BUFFER_BYTES: usize = 4 * 1024;

    let mut bytes = Vec::with_capacity(bytes_max);
    let mut buffer = [0_u8; READ_BUFFER_BYTES];
    loop {
        let bytes_remaining = bytes_max - bytes.len();
        if bytes_remaining == 0 {
            let mut overflow = [0_u8; 1];
            return match reader.read(&mut overflow)? {
                0 => Ok(bytes),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("command stdout exceeds {bytes_max} bytes"),
                )),
            };
        }

        let read_buffer_bytes = bytes_remaining.min(buffer.len());
        let bytes_read = reader.read(&mut buffer[..read_buffer_bytes])?;
        if bytes_read == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..bytes_read]);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Cursor, Write},
        path::Path,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    use super::{COMMAND_POLL_INTERVAL, read_bytes_bounded, run_command_with_bounded_stdout_in};

    const STDOUT_BYTES_MAX: usize = 16 * 1024;
    const STDOUT_CLOSED_CHILD_MARKER: &str = "stdout-closed-child.marker";
    const STDOUT_HOLDER_PARENT_MARKER: &str = "stdout-holder-parent.marker";
    const STDOUT_HOLDER_DESCENDANT_MARKER: &str = "stdout-holder-descendant.marker";
    const STDOUT_HOLDER_DESCENDANT_PID: &str = "stdout-holder-descendant.pid";

    #[test]
    fn bounded_reader_accepts_exactly_the_configured_byte_limit() {
        let bytes = vec![b'x'; STDOUT_BYTES_MAX];

        assert_eq!(
            read_bytes_bounded(Cursor::new(bytes.clone()), STDOUT_BYTES_MAX)
                .expect("bounded bytes"),
            bytes,
        );
    }

    #[test]
    fn bounded_reader_rejects_one_byte_over_the_configured_limit() {
        let bytes = vec![b'x'; STDOUT_BYTES_MAX + 1];

        let error = read_bytes_bounded(Cursor::new(bytes), STDOUT_BYTES_MAX)
            .expect_err("oversized output fails");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn command_completion_retains_stdout_that_finishes_before_child_exit() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::write(temporary.path().join(STDOUT_CLOSED_CHILD_MARKER), b"ready")
            .expect("write child marker");
        let current_executable = std::env::current_exe().expect("current test executable");

        let output = run_command_with_bounded_stdout_in(
            &current_executable,
            &[
                "stdout_closer_waits_before_exit",
                "--ignored",
                "--nocapture",
            ],
            temporary.path(),
            Duration::from_secs(2),
            STDOUT_BYTES_MAX,
        )
        .expect("bounded command completes");

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("stdout-before-exit"));
    }

    #[test]
    #[ignore = "subprocess helper"]
    fn stdout_closer_waits_before_exit() {
        if !Path::new(STDOUT_CLOSED_CHILD_MARKER).is_file() {
            return;
        }
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "stdout-before-exit").expect("write stdout");
        stdout.flush().expect("flush stdout");
        drop(stdout);
        close_stdout();
        thread::sleep(COMMAND_POLL_INTERVAL.saturating_mul(4));
        std::process::exit(0);
    }

    #[test]
    fn command_completion_terminates_a_descendant_that_retains_stdout() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        fs::write(temporary.path().join(STDOUT_HOLDER_PARENT_MARKER), b"ready")
            .expect("write parent marker");
        let current_executable = std::env::current_exe().expect("current test executable");
        let started_at = Instant::now();

        let output = run_command_with_bounded_stdout_in(
            &current_executable,
            &[
                "stdout_holder_parent_exits_after_spawning_descendant",
                "--ignored",
                "--nocapture",
            ],
            temporary.path(),
            Duration::from_secs(2),
            STDOUT_BYTES_MAX,
        )
        .expect("bounded command completes");

        assert!(output.status.success());
        assert!(started_at.elapsed() < Duration::from_secs(2));
        assert!(String::from_utf8_lossy(&output.stdout).contains("configured-editor"));
        let descendant_pid = wait_for_file(
            &temporary.path().join(STDOUT_HOLDER_DESCENDANT_PID),
            Duration::from_secs(1),
        );
        assert_process_exits(descendant_pid, Duration::from_secs(1));
    }

    #[test]
    #[ignore = "subprocess helper"]
    #[allow(
        clippy::zombie_processes,
        reason = "the parent must exit while its descendant retains stdout"
    )]
    fn stdout_holder_parent_exits_after_spawning_descendant() {
        if !Path::new(STDOUT_HOLDER_PARENT_MARKER).is_file() {
            return;
        }
        fs::write(STDOUT_HOLDER_DESCENDANT_MARKER, b"ready").expect("write descendant marker");
        let child = Command::new(std::env::current_exe().expect("current test executable"))
            .args(["stdout_holder_descendant_waits", "--ignored", "--nocapture"])
            .current_dir(std::env::current_dir().expect("current directory"))
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn stdout-holding descendant");
        fs::write(STDOUT_HOLDER_DESCENDANT_PID, child.id().to_string())
            .expect("write descendant pid");
        println!("configured-editor");
    }

    #[test]
    #[ignore = "subprocess helper"]
    fn stdout_holder_descendant_waits() {
        if !Path::new(STDOUT_HOLDER_DESCENDANT_MARKER).is_file() {
            return;
        }
        thread::sleep(Duration::from_secs(30));
    }

    fn wait_for_file(path: &Path, timeout: Duration) -> u32 {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(value) = fs::read_to_string(path) {
                return value.trim().parse().expect("numeric process id");
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {}", path.display());
    }

    fn assert_process_exits(process_id: u32, timeout: Duration) {
        if std::env::consts::OS != "linux" {
            return;
        }
        let process_path = std::path::PathBuf::from(format!("/proc/{process_id}"));
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !process_path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("process {process_id} remained after bounded command completion");
    }

    #[cfg(unix)]
    fn close_stdout() {
        use std::os::fd::{AsRawFd, FromRawFd};

        let stdout = std::io::stdout();
        let stdout_file_descriptor = stdout.as_raw_fd();
        // SAFETY: this subprocess exits without using stdout after ownership of
        // its valid standard-output descriptor is transferred and dropped.
        unsafe {
            drop(fs::File::from_raw_fd(stdout_file_descriptor));
        }
    }

    #[cfg(windows)]
    fn close_stdout() {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};

        let stdout = std::io::stdout();
        let stdout_handle = stdout.as_raw_handle();
        // SAFETY: this subprocess exits without using stdout after ownership of
        // its valid standard-output handle is transferred and dropped.
        unsafe {
            drop(fs::File::from_raw_handle(stdout_handle));
        }
    }
}
