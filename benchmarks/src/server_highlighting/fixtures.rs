use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};

use super::{FixtureIdentity, HighlightWorkload};
use crate::desktop_scroll;

const SYNTHETIC_LINE_COUNT: usize = 2_000;
const FULL_LANGUAGE_LINE_COUNT: usize = 400;
const GIT_DIAGNOSTIC_BYTES_MAX: usize = 8 * 1_024;
const IDENTITY_NAME: &str = "Server Highlighting Fixture";
const IDENTITY_EMAIL: &str = "server-highlighting@example.invalid";
const BASE_TIMESTAMP: &str = "2026-02-01T12:00:00+00:00";
const CHANGE_TIMESTAMP: &str = "2026-02-02T12:00:00+00:00";
const GIT_ENVIRONMENT_VARIABLES_REMOVED: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_WORK_TREE",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedFixture {
    pub repository: PathBuf,
    pub identity: FixtureIdentity,
}

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("{operation} `{path}`")]
    FileSystem {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Git {operation} failed with exit {exit_code}: {stderr}")]
    Git {
        operation: String,
        exit_code: i32,
        stderr: String,
    },
    #[error("hydrate the mixed server highlighting fixture")]
    MixedFixture(#[source] desktop_scroll::DesktopScrollFixtureError),
    #[error("server highlighting fixture is invalid: {reason}")]
    Invalid { reason: String },
}

pub fn materialize_fixture(
    workload: HighlightWorkload,
    destination: &Path,
) -> Result<MaterializedFixture, FixtureError> {
    match workload {
        HighlightWorkload::Mixed => materialize_mixed(destination),
        HighlightWorkload::Plain | HighlightWorkload::Rust | HighlightWorkload::Full => {
            materialize_synthetic(workload, destination)
        }
    }
}

pub fn synthetic_fixture_identity(
    workload: HighlightWorkload,
) -> Result<FixtureIdentity, FixtureError> {
    if workload == HighlightWorkload::Mixed {
        return Err(invalid(
            "mixed identity comes from the committed desktop fixture",
        ));
    }
    let temporary = tempfile::Builder::new()
        .prefix("server-highlighting-identity-")
        .tempdir()
        .map_err(file_system_error(
            "create temporary fixture",
            Path::new("/tmp"),
        ))?;
    materialize_synthetic(workload, &temporary.path().join(workload.name()))
        .map(|fixture| fixture.identity)
}

fn materialize_mixed(destination: &Path) -> Result<MaterializedFixture, FixtureError> {
    let manifest = desktop_scroll::hydrate_fixture(&desktop_scroll::fixture_root(), destination)
        .map_err(FixtureError::MixedFixture)?;
    let range = format!("HEAD~{}..HEAD", manifest.workload.commit_count);
    let diff = git_output(
        destination,
        "capture mixed fixture diff",
        ["diff", range.as_str()],
    )?;
    Ok(MaterializedFixture {
        repository: destination.to_path_buf(),
        identity: FixtureIdentity {
            workload: HighlightWorkload::Mixed,
            name: manifest.fixture_name,
            source_sha256: sha256_hex(&diff),
            file_count: manifest.workload.distinct_file_count,
            changed_line_count: usize::try_from(
                manifest
                    .workload
                    .additions
                    .saturating_add(manifest.workload.deletions),
            )
            .map_err(|_| invalid("mixed changed-line count exceeds usize"))?,
            changed_bytes: manifest.workload.compact_diff_bytes,
            last_commit_count: u32::try_from(manifest.workload.commit_count)
                .map_err(|_| invalid("mixed commit count exceeds u32"))?,
        },
    })
}

fn materialize_synthetic(
    workload: HighlightWorkload,
    destination: &Path,
) -> Result<MaterializedFixture, FixtureError> {
    prepare_destination(destination)?;
    git_run(
        destination,
        "initialize repository",
        ["init", "-q", "-b", "main"],
    )?;
    for (key, value) in [
        ("user.name", IDENTITY_NAME),
        ("user.email", IDENTITY_EMAIL),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
        ("core.fileMode", "false"),
        ("diff.renames", "false"),
    ] {
        git_run(destination, "configure repository", ["config", key, value])?;
    }
    let files = synthetic_files(workload)?;
    write_file_versions(destination, &files, false)?;
    commit_all(
        destination,
        "fixture: establish highlighting baseline",
        BASE_TIMESTAMP,
    )?;
    write_file_versions(destination, &files, true)?;
    commit_all(
        destination,
        "fixture: exercise server highlighting",
        CHANGE_TIMESTAMP,
    )?;

    let diff = git_output(
        destination,
        "capture synthetic fixture diff",
        ["diff", "HEAD~1..HEAD"],
    )?;
    let numstat = git_output(
        destination,
        "measure synthetic changed lines",
        ["diff", "--numstat", "HEAD~1..HEAD"],
    )?;
    let changed_line_count = parse_changed_lines(&numstat)?;
    Ok(MaterializedFixture {
        repository: destination.to_path_buf(),
        identity: FixtureIdentity {
            workload,
            name: format!("server-highlighting-{}-v1", workload.name()),
            source_sha256: sha256_hex(&diff),
            file_count: files.len(),
            changed_line_count,
            changed_bytes: diff.len() as u64,
            last_commit_count: 1,
        },
    })
}

fn synthetic_files(
    workload: HighlightWorkload,
) -> Result<BTreeMap<&'static str, (String, String)>, FixtureError> {
    match workload {
        HighlightWorkload::Plain => Ok(BTreeMap::from([(
            "src/benchmark.fixture",
            (plain_source(0), plain_source(10_000)),
        )])),
        HighlightWorkload::Rust => Ok(BTreeMap::from([(
            "src/benchmark.rs",
            (
                rust_source(SYNTHETIC_LINE_COUNT, 0),
                rust_source(SYNTHETIC_LINE_COUNT, 10_000),
            ),
        )])),
        HighlightWorkload::Full => Ok(BTreeMap::from([
            (
                "src/benchmark.js",
                (javascript_source(0), javascript_source(10_000)),
            ),
            (
                "src/benchmark.ts",
                (typescript_source(0), typescript_source(10_000)),
            ),
            (
                "src/benchmark.py",
                (python_source(0), python_source(10_000)),
            ),
            (
                "src/benchmark.rs",
                (
                    rust_source(FULL_LANGUAGE_LINE_COUNT, 0),
                    rust_source(FULL_LANGUAGE_LINE_COUNT, 10_000),
                ),
            ),
            (
                "docs/benchmark.md",
                (markdown_source(0), markdown_source(10_000)),
            ),
            ("web/benchmark.html", (html_source(0), html_source(10_000))),
            (
                "config/benchmark.yaml",
                (yaml_source(0), yaml_source(10_000)),
            ),
        ])),
        HighlightWorkload::Mixed => Err(invalid("mixed fixture is committed separately")),
    }
}

fn plain_source(offset: usize) -> String {
    generated_lines(SYNTHETIC_LINE_COUNT, |line| {
        format!(
            "record-{line:04}: benchmark payload value {:08} with stable plain text columns\n",
            line + offset
        )
    })
}

fn rust_source(lines: usize, offset: usize) -> String {
    generated_lines(lines, |line| {
        format!(
            "pub fn benchmark_{line:04}(input: usize) -> usize {{ input.saturating_add({}) }}\n",
            line + offset
        )
    })
}

fn javascript_source(offset: usize) -> String {
    generated_lines(FULL_LANGUAGE_LINE_COUNT, |line| {
        format!(
            "export function benchmark{line}(value) {{ return value + {}; }}\n",
            line + offset
        )
    })
}

fn typescript_source(offset: usize) -> String {
    generated_lines(FULL_LANGUAGE_LINE_COUNT, |line| {
        format!(
            "export const benchmark{line} = (value: number): number => value + {};\n",
            line + offset
        )
    })
}

fn python_source(offset: usize) -> String {
    generated_lines(FULL_LANGUAGE_LINE_COUNT, |line| {
        format!(
            "def benchmark_{line}(value: int) -> int: return value + {}\n",
            line + offset
        )
    })
}

fn markdown_source(offset: usize) -> String {
    let mut source = String::from("# Server highlighting benchmark\n\n```rust\n");
    source.push_str(&rust_source(FULL_LANGUAGE_LINE_COUNT, offset));
    source.push_str("```\n\nInline `let highlighted = true;` content.\n");
    source
}

fn html_source(offset: usize) -> String {
    let mut source = String::from("<!doctype html>\n<html><body><script>\n");
    source.push_str(&javascript_source(offset));
    source.push_str("</script></body></html>\n");
    source
}

fn yaml_source(offset: usize) -> String {
    generated_lines(FULL_LANGUAGE_LINE_COUNT, |line| {
        format!(
            "benchmark_{line}: {{ enabled: true, value: {} }}\n",
            line + offset
        )
    })
}

fn generated_lines(lines: usize, mut line: impl FnMut(usize) -> String) -> String {
    let mut output = String::with_capacity(lines.saturating_mul(80));
    for index in 0..lines {
        output.push_str(&line(index));
    }
    output
}

fn prepare_destination(destination: &Path) -> Result<(), FixtureError> {
    if destination.exists() {
        return Err(invalid(format!(
            "destination already exists: {}",
            destination.display()
        )));
    }
    fs::create_dir_all(destination)
        .map_err(file_system_error("create fixture destination", destination))
}

fn write_file_versions(
    root: &Path,
    files: &BTreeMap<&str, (String, String)>,
    changed: bool,
) -> Result<(), FixtureError> {
    for (relative, versions) in files {
        let path = root.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| invalid(format!("fixture path has no parent: {relative}")))?;
        fs::create_dir_all(parent)
            .map_err(file_system_error("create fixture file parent", parent))?;
        let contents = if changed { &versions.1 } else { &versions.0 };
        fs::write(&path, contents).map_err(file_system_error("write fixture file", &path))?;
    }
    Ok(())
}

fn commit_all(root: &Path, subject: &str, timestamp: &str) -> Result<(), FixtureError> {
    git_run(root, "stage fixture files", ["add", "-A"])?;
    git_output_with_identity(
        root,
        "commit fixture files",
        ["commit", "-q", "--no-gpg-sign", "-m", subject],
        timestamp,
    )?;
    Ok(())
}

fn parse_changed_lines(numstat: &[u8]) -> Result<usize, FixtureError> {
    let text =
        std::str::from_utf8(numstat).map_err(|_| invalid("Git numstat output is not UTF-8"))?;
    text.lines().try_fold(0_usize, |total, line| {
        let mut fields = line.split('\t');
        let additions = fields
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| invalid("Git numstat additions are invalid"))?;
        let deletions = fields
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| invalid("Git numstat deletions are invalid"))?;
        total
            .checked_add(additions)
            .and_then(|value| value.checked_add(deletions))
            .ok_or_else(|| invalid("fixture changed-line count overflows"))
    })
}

fn git_run<I, S>(root: &Path, operation: &str, arguments: I) -> Result<(), FixtureError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    git_output(root, operation, arguments).map(|_| ())
}

fn git_output<I, S>(root: &Path, operation: &str, arguments: I) -> Result<Vec<u8>, FixtureError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    git_command(root, operation, arguments, None)
}

fn git_output_with_identity<I, S>(
    root: &Path,
    operation: &str,
    arguments: I,
    timestamp: &str,
) -> Result<Vec<u8>, FixtureError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    git_command(root, operation, arguments, Some(timestamp))
}

fn git_command<I, S>(
    root: &Path,
    operation: &str,
    arguments: I,
    timestamp: Option<&str>,
) -> Result<Vec<u8>, FixtureError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("TZ", "UTC");
    for name in GIT_ENVIRONMENT_VARIABLES_REMOVED {
        command.env_remove(name);
    }
    if let Some(timestamp) = timestamp {
        command
            .env("GIT_AUTHOR_NAME", IDENTITY_NAME)
            .env("GIT_AUTHOR_EMAIL", IDENTITY_EMAIL)
            .env("GIT_AUTHOR_DATE", timestamp)
            .env("GIT_COMMITTER_NAME", IDENTITY_NAME)
            .env("GIT_COMMITTER_EMAIL", IDENTITY_EMAIL)
            .env("GIT_COMMITTER_DATE", timestamp);
    }
    let output = command
        .output()
        .map_err(file_system_error("start deterministic Git command", root))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(FixtureError::Git {
        operation: operation.to_owned(),
        exit_code: output.status.code().unwrap_or(-1),
        stderr: bounded_diagnostic(&output.stderr),
    })
}

fn file_system_error<'path>(
    operation: &'static str,
    path: &'path Path,
) -> impl FnOnce(io::Error) -> FixtureError + 'path {
    move |source| FixtureError::FileSystem {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

fn invalid(reason: impl Into<String>) -> FixtureError {
    FixtureError::Invalid {
        reason: reason.into(),
    }
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    let end = bytes.len().min(GIT_DIAGNOSTIC_BYTES_MAX);
    let suffix = if bytes.len() > end {
        "...[truncated]"
    } else {
        ""
    };
    format!("{}{suffix}", String::from_utf8_lossy(&bytes[..end]).trim())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_fixture_identities_are_repeatable() {
        for workload in [
            HighlightWorkload::Plain,
            HighlightWorkload::Rust,
            HighlightWorkload::Full,
        ] {
            let first = synthetic_fixture_identity(workload).unwrap();
            let second = synthetic_fixture_identity(workload).unwrap();

            assert_eq!(first, second, "{workload}");
            assert_eq!(first.source_sha256.len(), 64);
            assert!(first.changed_line_count > 0);
        }
    }

    #[test]
    fn full_fixture_contains_every_supported_extension_and_injections() {
        let files = synthetic_files(HighlightWorkload::Full).unwrap();

        for path in [
            "src/benchmark.js",
            "src/benchmark.ts",
            "src/benchmark.py",
            "src/benchmark.rs",
            "docs/benchmark.md",
            "web/benchmark.html",
            "config/benchmark.yaml",
        ] {
            assert!(files.contains_key(path), "{path}");
        }
        assert!(files["docs/benchmark.md"].1.contains("```rust"));
        assert!(files["web/benchmark.html"].1.contains("<script>"));
    }
}
