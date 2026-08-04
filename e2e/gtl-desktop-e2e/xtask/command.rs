use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use command_group::CommandGroup;

use super::{terminate_and_reap, wait_for_exit};

#[test]
fn failed_parent_cleanup_terminates_its_remaining_process_group() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let child_pid_path = temporary_directory.path().join("child.pid");
    let mut command = Command::new("sh");
    command
        .env("GTL_E2E_CHILD_PID_PATH", &child_pid_path)
        .args([
            "-c",
            "sleep 2147483647 & printf '%s' \"$!\" > \"$GTL_E2E_CHILD_PID_PATH\"; exit 7",
        ]);
    let mut child = command.group_spawn().unwrap();
    let status = wait_for_exit(
        &mut child,
        Duration::from_secs(1),
        "failing process-group fixture",
    )
    .unwrap();
    let child_pid = fs::read_to_string(&child_pid_path)
        .unwrap()
        .parse::<u32>()
        .unwrap();

    assert!(!status.success());
    terminate_and_reap(&mut child, "failing process-group fixture").unwrap();
    assert!(wait_for_process_exit(child_pid, Duration::from_secs(1)));
}

fn wait_for_process_exit(pid: u32, duration: Duration) -> bool {
    let process_path = PathBuf::from(format!("/proc/{pid}"));
    let deadline = Instant::now() + duration;
    loop {
        if !process_path.exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
