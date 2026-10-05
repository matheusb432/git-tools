//! Sends one github.com REST request through the GitHub CLI with the user's credentials.

use std::{
    ffi::OsString,
    io::Read as _,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context as _, bail};
use gtl_models::diffs::DIFF_TEXT_BYTES_MAX;
use gtl_wire::v1;

use crate::failure::Refusal;

const GITHUB_HOST: &str = "github.com";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
const HEADER_BYTES_MAX: usize = 64 * 1024;
const STDERR_BYTES_MAX: usize = 64 * 1024;
const EXIT_CODE_AUTHENTICATION_REQUIRED: i32 = 4;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// GitHub's answer, including refusals, which the server interprets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitHubResponse {
    pub(crate) status: u16,
    pub(crate) rate_limit_remaining: Option<u64>,
    pub(crate) body: Vec<u8>,
}

/// Runs `gh api` from `PATH`.
pub(crate) struct GhApi {
    program: OsString,
    timeout: Duration,
    body_bytes_max: usize,
}

impl Default for GhApi {
    fn default() -> Self {
        Self {
            program: OsString::from("gh"),
            timeout: RESPONSE_TIMEOUT,
            body_bytes_max: DIFF_TEXT_BYTES_MAX,
        }
    }
}

impl GhApi {
    /// Sends `request` once; `gh` makes no other request.
    pub(crate) fn get(&self, request: &v1::GitHubApiRequest) -> anyhow::Result<GitHubResponse> {
        let output = self.run(request)?;
        if output.stdout.starts_with(b"HTTP/") {
            let response = parse_included_response(output.stdout)?;
            if response.body.len() > self.body_bytes_max {
                bail!(too_large(self.body_bytes_max));
            }
            return Ok(response);
        }
        if output.status.code() == Some(EXIT_CODE_AUTHENTICATION_REQUIRED) {
            return Err(Refusal(
                "The GitHub CLI is not signed in. Run `gh auth login` and try again.".to_owned(),
            )
            .into());
        }
        bail!(
            "gh api exited with {} without a response: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    fn command(&self, request: &v1::GitHubApiRequest) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args([
                "api",
                "--hostname",
                GITHUB_HOST,
                "--method",
                "GET",
                "--include",
            ])
            .arg("--header")
            .arg(format!("Accept: {}", request.accept))
            .arg(&request.path)
            // gh otherwise sends usage telemetry and checks for new releases in the background.
            .env("GH_TELEMETRY", "false")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, request: &v1::GitHubApiRequest) -> anyhow::Result<ProcessOutput> {
        let mut child = match self.command(request).spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Refusal(
                    "The GitHub CLI `gh` is not on PATH. Install it to diff GitHub comparisons."
                        .to_owned(),
                )
                .into());
            }
            Err(error) => return Err(error).context("starting gh api"),
        };
        let stdout_overflowed = Arc::new(AtomicBool::new(false));
        let stdout = read_capped(
            child.stdout.take().context("gh api stdout")?,
            self.body_bytes_max + HEADER_BYTES_MAX,
            Some(Arc::clone(&stdout_overflowed)),
        );
        let stderr = read_capped(
            child.stderr.take().context("gh api stderr")?,
            STDERR_BYTES_MAX,
            None,
        );
        let started = Instant::now();
        let status = loop {
            if stdout_overflowed.load(Ordering::Relaxed) {
                stop(&mut child);
                bail!(too_large(self.body_bytes_max));
            }
            if started.elapsed() > self.timeout {
                stop(&mut child);
                bail!(
                    "GitHub did not answer within {} seconds. Try again.",
                    self.timeout.as_secs()
                );
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(error) => {
                    stop(&mut child);
                    return Err(error).context("waiting for gh api");
                }
            }
        };
        let stdout = join_reader(stdout)?;
        if stdout_overflowed.load(Ordering::Relaxed) {
            bail!(too_large(self.body_bytes_max));
        }
        Ok(ProcessOutput {
            status,
            stdout,
            stderr: join_reader(stderr)?,
        })
    }
}

struct ProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn too_large(body_bytes_max: usize) -> String {
    format!(
        "GitHub's diff is larger than {} MiB.",
        body_bytes_max / (1024 * 1024)
    )
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads at most `bytes_max` bytes, then closes the pipe and raises `overflowed` if more followed.
fn read_capped(
    pipe: impl std::io::Read + Send + 'static,
    bytes_max: usize,
    overflowed: Option<Arc<AtomicBool>>,
) -> JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.take(bytes_max as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > bytes_max {
            bytes.truncate(bytes_max);
            if let Some(overflowed) = overflowed {
                overflowed.store(true, Ordering::Relaxed);
            }
        }
        Ok(bytes)
    })
}

fn join_reader(reader: JoinHandle<std::io::Result<Vec<u8>>>) -> anyhow::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("gh api output reader panicked"))?
        .context("reading gh api output")
}

fn parse_included_response(mut stdout: Vec<u8>) -> anyhow::Result<GitHubResponse> {
    let status_line_end = stdout
        .iter()
        .position(|&byte| byte == b'\n')
        .context("gh api response ended inside its headers")?;
    let status_line = String::from_utf8_lossy(&stdout[..status_line_end]);
    let status_line = status_line.trim_end_matches('\r');
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .with_context(|| format!("gh api printed an invalid status line: {status_line}"))?;
    let mut rate_limit_remaining = None;
    let mut line_start = status_line_end + 1;
    let body_start = loop {
        let line_end = stdout[line_start..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|offset| line_start + offset)
            .context("gh api response ended inside its headers")?;
        let line = String::from_utf8_lossy(&stdout[line_start..line_end]);
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break line_end + 1;
        } else if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("x-ratelimit-remaining")
        {
            rate_limit_remaining = value.trim().parse().ok();
        }
        line_start = line_end + 1;
    };
    let body = stdout.split_off(body_start);
    Ok(GitHubResponse {
        status,
        rate_limit_remaining,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF_BODY: &str = "diff --git a/x b/x\r\n--- a/x\r\n+++ b/x\r\n\r\n";

    #[test]
    fn included_output_separates_status_headers_and_body() {
        let stdout = format!(
            "HTTP/2.0 200 OK\nContent-Type: application/vnd.github.diff\r\nX-Ratelimit-Remaining: 4991\r\n\r\n{DIFF_BODY}"
        );

        let response = parse_included_response(stdout.into_bytes()).unwrap();

        assert_eq!(
            response,
            GitHubResponse {
                status: 200,
                rate_limit_remaining: Some(4991),
                body: DIFF_BODY.as_bytes().to_vec(),
            }
        );
    }

    #[test]
    fn included_output_without_a_header_terminator_is_rejected() {
        assert!(
            parse_included_response(b"HTTP/2.0 404 Not Found\nServer: x\r\n".to_vec()).is_err()
        );
        assert!(parse_included_response(b"HTTP/2.0 abc\n\r\n".to_vec()).is_err());
    }

    #[cfg(unix)]
    mod process {
        use std::os::unix::fs::PermissionsExt as _;

        use super::*;

        fn request() -> v1::GitHubApiRequest {
            v1::GitHubApiRequest {
                path: "repos/example-org/widget/compare/v1...v2".to_owned(),
                accept: "application/vnd.github.diff".to_owned(),
            }
        }

        fn fake_gh(directory: &tempfile::TempDir, script: &str, timeout: Duration) -> GhApi {
            let program = directory.path().join("gh");
            std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            GhApi {
                program: program.into_os_string(),
                timeout,
                body_bytes_max: 1024,
            }
        }

        fn refusal(error: &anyhow::Error) -> Option<&str> {
            error
                .downcast_ref::<Refusal>()
                .map(|Refusal(message)| message.as_str())
        }

        #[test]
        fn one_request_disables_background_traffic_and_returns_the_response() {
            let directory = tempfile::tempdir().unwrap();
            let log = directory.path().join("log");
            let gh = fake_gh(
                &directory,
                &format!(
                    "printf '%s\\n' \"$*\" \"$GH_TELEMETRY $GH_NO_UPDATE_NOTIFIER $GH_PROMPT_DISABLED\" >> '{}'\nprintf 'HTTP/2.0 404 Not Found\\nX-Ratelimit-Remaining: 7\\r\\n\\r\\n{{\"message\":\"Not Found\"}}'\nexit 1",
                    log.display()
                ),
                Duration::from_secs(10),
            );

            let response = gh.get(&request()).unwrap();

            assert_eq!(
                std::fs::read_to_string(&log).unwrap(),
                "api --hostname github.com --method GET --include --header Accept: application/vnd.github.diff repos/example-org/widget/compare/v1...v2\nfalse 1 1\n"
            );
            assert_eq!(
                response,
                GitHubResponse {
                    status: 404,
                    rate_limit_remaining: Some(7),
                    body: br#"{"message":"Not Found"}"#.to_vec(),
                }
            );
        }

        #[test]
        fn missing_programs_and_credentials_are_refusals() {
            let directory = tempfile::tempdir().unwrap();
            let unauthenticated = fake_gh(&directory, "exit 4", Duration::from_secs(10))
                .get(&request())
                .unwrap_err();
            let missing = GhApi {
                program: directory.path().join("absent").into_os_string(),
                ..GhApi::default()
            }
            .get(&request())
            .unwrap_err();

            assert!(
                refusal(&unauthenticated).is_some_and(|message| message.contains("gh auth login"))
            );
            assert!(refusal(&missing).is_some_and(|message| message.contains("not on PATH")));
        }

        #[test]
        fn slow_and_oversized_responses_are_stopped() {
            let directory = tempfile::tempdir().unwrap();
            let slow = fake_gh(&directory, "exec sleep 5", Duration::from_millis(50))
                .get(&request())
                .unwrap_err();
            let oversized = fake_gh(
                &directory,
                "printf 'HTTP/2.0 200 OK\\n\\r\\n'; head -c 2048 /dev/zero",
                Duration::from_secs(10),
            )
            .get(&request())
            .unwrap_err();
            let flooding = fake_gh(&directory, "exec cat /dev/zero", Duration::from_secs(10))
                .get(&request())
                .unwrap_err();

            assert!(slow.to_string().contains("did not answer"), "{slow}");
            assert!(oversized.to_string().contains("larger than"), "{oversized}");
            assert!(flooding.to_string().contains("larger than"), "{flooding}");
        }
    }
}
