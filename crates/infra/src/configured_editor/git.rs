use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use wait_timeout::ChildExt;

const GIT_EDITOR_QUERY_TIMEOUT: Duration = Duration::from_secs(3);
const GIT_EDITOR_COMMAND_BYTES_MAX: usize = 16 * 1024;
const GIT_EDITOR_READ_BUFFER_BYTES: usize = 4 * 1024;

pub(super) fn read_configured_command(repository_root: &Path) -> Result<String> {
    let mut child = Command::new("git")
        .args(["var", "GIT_EDITOR"])
        .current_dir(repository_root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("start Git editor query in {}", repository_root.display()))?;
    let stdout = child
        .stdout
        .take()
        .context("Git editor query did not provide stdout")?;
    let reader = thread::spawn(move || read_configured_command_bytes(stdout));

    let status = match child.wait_timeout(GIT_EDITOR_QUERY_TIMEOUT) {
        Ok(status) => status,
        Err(error) => {
            let _ = terminate_reap_and_join(&mut child, reader);
            return Err(error).context("wait for Git editor query");
        }
    };

    if let Some(status) = status {
        let command_bytes = join_reader(reader)?;
        if !status.success() {
            bail!("Git editor query exited with {status}");
        }

        let command =
            String::from_utf8(command_bytes).context("Git editor command is not UTF-8")?;
        let command = command.trim();
        if command.is_empty() {
            bail!("Git editor command is empty");
        }

        Ok(command.to_owned())
    } else {
        terminate_reap_and_join(&mut child, reader)?;
        bail!("Git editor query timed out after 3 seconds");
    }
}

fn terminate_reap_and_join(
    child: &mut std::process::Child,
    reader: JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<()> {
    let kill_result = child.kill().context("terminate Git editor query");
    let reap_result = child.wait().context("reap Git editor query");
    let reader_result = join_reader(reader);
    kill_result?;
    reap_result?;
    drop(reader_result);
    Ok(())
}

fn join_reader(reader: JoinHandle<std::io::Result<Vec<u8>>>) -> Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("Git editor query reader thread panicked"))?
        .context("read Git editor command")
}

fn read_configured_command_bytes(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(GIT_EDITOR_COMMAND_BYTES_MAX);
    let mut buffer = [0_u8; GIT_EDITOR_READ_BUFFER_BYTES];

    loop {
        let bytes_remaining = GIT_EDITOR_COMMAND_BYTES_MAX - bytes.len();
        if bytes_remaining == 0 {
            let mut overflow = [0_u8; 1];
            return match reader.read(&mut overflow)? {
                0 => Ok(bytes),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Git editor command exceeds 16 KiB",
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
        process::{Command, Stdio},
        thread,
    };

    use super::{
        GIT_EDITOR_COMMAND_BYTES_MAX, read_configured_command_bytes, terminate_reap_and_join,
    };

    #[test]
    fn configured_command_bytes_accepts_exactly_sixteen_kibibytes() {
        let bytes = vec![b'x'; GIT_EDITOR_COMMAND_BYTES_MAX];
        assert_eq!(
            read_configured_command_bytes(std::io::Cursor::new(bytes.clone()))
                .expect("bounded bytes"),
            bytes,
        );
    }

    #[test]
    fn configured_command_bytes_rejects_one_byte_over_the_limit() {
        let bytes = vec![b'x'; GIT_EDITOR_COMMAND_BYTES_MAX + 1];
        let error = read_configured_command_bytes(std::io::Cursor::new(bytes))
            .expect_err("oversized command fails");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn cleanup_reaps_a_blocking_git_child_and_joins_its_reader() {
        let mut child = Command::new("git")
            .args(["hash-object", "--stdin"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start blocking Git child");
        let stdout = child.stdout.take().expect("Git child stdout");
        let reader = thread::spawn(move || read_configured_command_bytes(stdout));

        terminate_reap_and_join(&mut child, reader).expect("clean up Git child");

        assert!(child.try_wait().expect("inspect Git child").is_some());
    }
}
