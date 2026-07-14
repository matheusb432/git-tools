use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "run one repository gate with terse, durable output")]
struct Cli {
    /// Stream the command output while retaining the durable log.
    #[arg(long)]
    verbose: bool,
    /// Stable filesystem-safe scope used for the log and RESULT record.
    scope: String,
    /// Bash command to execute as one argument.
    command: String,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Scope(String);

impl Scope {
    fn parse(raw: String) -> Result<Self> {
        let safe = !raw.is_empty()
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if !safe {
            bail!("filesystem-safe scope must contain only ASCII letters, digits, '-' or '_'");
        }
        Ok(Self(raw))
    }

    fn log_path(&self) -> PathBuf {
        Path::new(".artifacts")
            .join("logs")
            .join(format!("{}.log", self.0))
    }
}

fn run(cli: Cli) -> Result<()> {
    let scope = Scope::parse(cli.scope)?;
    let log_path = scope.log_path();
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create gate log directory {}", parent.display()))?;
    }

    let mut log = File::create(&log_path)
        .with_context(|| format!("create gate log {}", log_path.display()))?;
    let started = Instant::now();
    let mut child = Command::new("bash")
        .args(["-c", "exec 2>&1; bash -c \"$1\"", "gate", &cli.command])
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
        if cli.verbose {
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
    if !cli.verbose {
        eprintln!("--- failure tail: {} ---", log_path.display());
        eprint!("{}", failure_tail(&captured, 40));
    }
    bail!(
        "gate `{}` failed (exit {})",
        scope.0,
        status.code().unwrap_or(-1)
    )
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
        assert!(Scope::parse("quality_frontend".into()).is_ok());
        assert!(Scope::parse("../quality".into()).is_err());
        assert!(Scope::parse("quality/frontend".into()).is_err());
    }

    #[test]
    fn failure_tail_keeps_only_the_requested_lines() {
        assert_eq!(failure_tail(b"one\ntwo\nthree\n", 2), "two\nthree\n");
    }
}
