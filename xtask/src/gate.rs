//! Runs one repository gate command with terse, durable output: capture the command's combined
//! stdout and stderr into `<root>/.artifacts/logs/<scope>.log`, print a one-line PASS/FAIL summary
//! plus the parsed `RESULT` contract line, and tail the log to stderr on failure.
//!
//! Folded in from the standalone `gate` binary [`crate::process::gate`] used to shell out to via
//! `cargo run -p gate --`; running the capture in-process drops that extra build-and-spawn hop.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail};

/// Runs `command` through `bash -c` in `root` (defaulting to `.`), capturing combined output into
/// `<root>/.artifacts/logs/<scope>.log`. `verbose` also streams output live to stderr; otherwise
/// only a failure tails the log.
pub(crate) fn run(root: Option<&Path>, scope: &str, command: &str, verbose: bool) -> Result<()> {
    let root = root.unwrap_or(Path::new("."));
    let scope = Scope::parse(scope)?;
    let log_path = root.join(scope.relative_log_path());
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create gate log directory {}", parent.display()))?;
    }

    let mut log = File::create(&log_path)
        .with_context(|| format!("create gate log {}", log_path.display()))?;
    let started = Instant::now();
    let mut child = Command::new("bash")
        .args(["-c", "exec 2>&1; bash -c \"$1\"", "gate", command])
        .current_dir(root)
        .stdout(Stdio::piped())
        .spawn()
        .context("start gate command through bash")?;
    let mut stdout = child.stdout.take().context("capture gate command output")?;
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stdout
            .read(&mut chunk)
            .context("read gate command output")?;
        if read == 0 {
            break;
        }
        let bytes = &chunk[..read];
        log.write_all(bytes).context("write durable gate log")?;
        captured.extend_from_slice(bytes);
        if verbose {
            std::io::stderr()
                .write_all(bytes)
                .context("stream verbose gate output")?;
        }
    }
    let status = child.wait().context("wait for gate command")?;
    let elapsed = started.elapsed().as_secs_f32();
    let result = if status.success() { "PASS" } else { "FAIL" };
    eprintln!(
        "{result:<4} {:<24} {elapsed:>7.2}s  {}",
        scope.0,
        log_path.display()
    );
    println!(
        "RESULT scope={} status={result} log={}",
        scope.0,
        log_path.display()
    );

    if status.success() {
        return Ok(());
    }
    if !verbose {
        eprintln!("--- failure tail: {} ---", log_path.display());
        eprint!("{}", failure_tail(&captured, 40));
    }
    bail!(
        "gate `{}` failed (exit {})",
        scope.0,
        status.code().unwrap_or(-1)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Scope(String);

impl Scope {
    fn parse(raw: &str) -> Result<Self> {
        let safe = !raw.is_empty()
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if !safe {
            bail!("filesystem-safe scope must contain only ASCII letters, digits, '-' or '_'");
        }
        Ok(Self(raw.to_string()))
    }

    fn relative_log_path(&self) -> PathBuf {
        Path::new(".artifacts")
            .join("logs")
            .join(format!("{}.log", self.0))
    }
}

fn failure_tail(output: &[u8], line_limit: usize) -> String {
    let output = String::from_utf8_lossy(output);
    let lines = output.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(line_limit);
    let mut tail = lines[start..].join("\n");
    if !tail.is_empty() {
        tail.push('\n');
    }
    tail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_rejects_path_syntax() {
        assert!(Scope::parse("quality_frontend").is_ok());
        assert!(Scope::parse("../quality").is_err());
        assert!(Scope::parse("quality/frontend").is_err());
    }

    #[test]
    fn failure_tail_keeps_only_the_requested_lines() {
        assert_eq!(failure_tail(b"one\ntwo\nthree\n", 2), "two\nthree\n");
    }

    #[test]
    fn passing_command_writes_a_durable_log() {
        let root = tempfile::tempdir().expect("temporary repository root");

        run(
            Some(root.path()),
            "quality",
            "printf 'hidden output\\n'",
            false,
        )
        .expect("gate run succeeds");

        assert_eq!(
            fs::read_to_string(root.path().join(".artifacts/logs/quality.log"))
                .expect("durable gate log"),
            "hidden output\n"
        );
    }

    #[test]
    fn failing_command_reports_a_tail_and_preserves_the_exit_failure() {
        let root = tempfile::tempdir().expect("temporary repository root");

        let error = run(
            Some(root.path()),
            "lint",
            "printf 'failure detail\\n'; exit 7",
            false,
        )
        .expect_err("nonzero exit propagates as an error");

        assert!(error.to_string().contains("lint"), "error: {error}");
        assert_eq!(
            fs::read_to_string(root.path().join(".artifacts/logs/lint.log"))
                .expect("durable gate log"),
            "failure detail\n"
        );
    }

    #[test]
    fn unsafe_scope_is_rejected_before_execution() {
        let root = tempfile::tempdir().expect("temporary repository root");

        let error =
            run(Some(root.path()), "../escape", "exit 0", false).expect_err("scope is unsafe");

        assert!(
            error.to_string().contains("filesystem-safe scope"),
            "error: {error}"
        );
        assert!(!root.path().join("../escape.log").exists());
    }
}
