use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let record_path = next_path(&mut arguments, "record")?;
    let release_path = next_path(&mut arguments, "release")?;
    let exit_path = next_path(&mut arguments, "exit")?;
    let working_directory = env::current_dir()?.to_string_lossy().into_owned();
    let arguments = arguments
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let record = serde_json::json!({
        "pid": std::process::id(),
        "working_directory": working_directory,
        "arguments": arguments,
    });
    let record_temporary_path = record_path.with_extension("tmp");
    // Publish the record only after serialization has completed.
    fs::write(&record_temporary_path, serde_json::to_vec(&record)?)?;
    fs::rename(record_temporary_path, record_path)?;

    let wait_result = wait_for_release(&release_path, Duration::from_secs(30));
    let exit_result = fs::write(exit_path, b"exited");
    wait_result?;
    exit_result?;
    Ok(())
}

fn next_path(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("editor recorder requires a {name} path").into())
}

fn wait_for_release(path: &Path, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.is_file() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(io::Error::new(
        ErrorKind::TimedOut,
        format!(
            "editor recorder release exceeded {} ms",
            timeout.as_millis()
        ),
    ))
}
