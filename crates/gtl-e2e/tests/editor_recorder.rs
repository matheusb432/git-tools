use std::{
    fs,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn recorder_remains_alive_until_released_and_then_exits() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let record_path = temporary.path().join("editor-record.json");
    let release_path = temporary.path().join("editor-release");
    let exit_path = temporary.path().join("editor-exit");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin!("editor-recorder"))
        .args([
            record_path.as_os_str(),
            release_path.as_os_str(),
            exit_path.as_os_str(),
            "--profile".as_ref(),
            "Viewer E2E".as_ref(),
        ])
        .spawn()
        .expect("start editor recorder");

    wait_for_path(&record_path, Duration::from_secs(2));
    assert!(
        child.try_wait().expect("inspect editor recorder").is_none(),
        "editor recorder exited before release"
    );

    fs::write(&release_path, b"release").expect("release editor recorder");
    wait_for_child_exit(&mut child, Duration::from_secs(2));
    assert!(exit_path.is_file());
}

fn wait_for_path(path: &std::path::Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.is_file() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {}", path.display());
}

fn wait_for_child_exit(child: &mut std::process::Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child.try_wait().expect("inspect editor recorder").is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.kill().expect("terminate stuck editor recorder");
    child.wait().expect("reap stuck editor recorder");
    panic!("editor recorder did not exit after release");
}
