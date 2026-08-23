use std::{
    io::Write as _,
    path::Path,
    process::{Child, Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};

use crate::task::Step;

pub(crate) fn run_step(step: &Step) -> Result<()> {
    let status = step_command(step).status()?;
    if !status.success() {
        bail!(
            "{} failed (exit {})",
            step.label(),
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

pub(crate) fn spawn_step(step: &Step) -> Result<Child> {
    step_command(step)
        .spawn()
        .with_context(|| format!("spawning {}", step.label()))
}

fn step_command(step: &Step) -> Command {
    let mut command = Command::new(step.program());
    command
        .args(step.arguments())
        .envs(step.environment().iter().map(|(key, value)| (key, value)));
    if let Some(directory) = step.current_directory() {
        command.current_dir(directory);
    }
    command
}

pub(crate) fn capture_bytes(label: &str, program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {})",
            output.status.code().unwrap_or(-1)
        );
    }
    Ok(output.stdout)
}

pub(crate) fn capture_bytes_with_stdin(
    label: &str,
    directory: Option<&Path>,
    program: &str,
    args: &[&str],
    input: &[u8],
) -> Result<Vec<u8>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }

    let mut child = command.spawn().with_context(|| format!("start {label}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("{label} did not expose stdin"))?;
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child
        .wait_with_output()
        .with_context(|| format!("wait for {label}"))?;
    writer
        .join()
        .map_err(|_| anyhow!("{label} stdin writer panicked"))?
        .with_context(|| format!("write input to {label}"))?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn development_child_remains_in_the_callers_process_group() {
        const CHILD_MARKER: &str = "XTASK_PROC_GROUP_CHILD";
        if std::env::var_os(CHILD_MARKER).is_some() {
            std::thread::sleep(std::time::Duration::from_secs(10));
            return;
        }

        let executable = std::env::current_exe().expect("test executable resolves");
        let step = Step::new(
            "process group child",
            executable.to_string_lossy().into_owned(),
            [
                "--exact",
                "process::tests::development_child_remains_in_the_callers_process_group",
            ],
        )
        .with_environment(CHILD_MARKER, "1");
        let parent_group = linux_process_group(std::process::id()).expect("parent process group");
        let mut child = spawn_step(&step).expect("development child starts");
        let child_group = linux_process_group(child.id());
        child.kill().expect("development child stops");
        child.wait().expect("development child is reaped");

        assert_eq!(child_group.expect("child process group"), parent_group);
    }

    #[cfg(target_os = "linux")]
    fn linux_process_group(process_id: u32) -> std::io::Result<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{process_id}/stat"))?;
        let (_, fields) = stat.rsplit_once(") ").ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "process stat has no command",
            )
        })?;
        let group = fields.split_whitespace().nth(2).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "process stat has no process group",
            )
        })?;
        group
            .parse()
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }
}
