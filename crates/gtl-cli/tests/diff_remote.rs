#![cfg(unix)]

use std::{os::unix::fs::PermissionsExt as _, path::Path};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use predicates::str::contains;

mod common;

const COMPARE_RESPONSE: &str = "HTTP/2.0 200 OK\\nContent-Type: application/vnd.github.diff\\r\\nX-Ratelimit-Remaining: 4999\\r\\n\\r\\ndiff --git a/src/widget.rs b/src/widget.rs\\nindex 1111111..2222222 100644\\n--- a/src/widget.rs\\n+++ b/src/widget.rs\\n@@ -1 +1 @@\\n-pub const SIZE: u8 = 1;\\n+pub const SIZE: u8 = 2;\\n";
const NOT_FOUND_RESPONSE: &str =
    "HTTP/2.0 404 Not Found\\nX-Ratelimit-Remaining: 4998\\r\\n\\r\\n{\"message\":\"Not Found\"}";

fn install_fake_gh(directory: &Path, log: &Path) -> Result<()> {
    let program = directory.join("gh");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\ncase \"$*\" in\n  *missing*) printf '{NOT_FOUND_RESPONSE}'; exit 1 ;;\n  *) printf '{COMPARE_RESPONSE}' ;;\nesac\n",
            log = log.display()
        ),
    )?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn requests(log: &Path) -> Result<Vec<String>> {
    Ok(std::fs::read_to_string(log)?
        .lines()
        .map(str::to_owned)
        .collect())
}

fn artifact_html(stdout: &[u8]) -> Result<String> {
    let stdout = String::from_utf8(stdout.to_vec())?;
    let path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("file://"))
        .context("render must print an artifact URL")?;
    Ok(std::fs::read_to_string(path)?)
}

fn remote_diff(origin: &str, range: &str, extra: &[&str]) -> Command {
    let mut command = Command::new(common::cli_binary());
    command
        .args(["diff", "--raw", "--remote", origin, range])
        .args(extra);
    command
}

const WIDGET: &str = "https://github.com/example-org/widget";
const WIDGET_RANGE: &str = "v1.0.0...v1.1.0";
const WIDGET_REQUEST: &str = "api --hostname github.com --method GET --include --header Accept: application/vnd.github.diff repos/example-org/widget/compare/v1.0.0...v1.1.0";

#[test]
fn remote_diffs_are_fetched_once_and_reused_until_refreshed() -> Result<()> {
    let tools = tempfile::tempdir()?;
    let log = tools.path().join("gh.log");
    install_fake_gh(tools.path(), &log)?;
    let inherited_path = std::env::var_os("PATH").context("missing executable path")?;
    let executable_path = std::env::join_paths(
        std::iter::once(tools.path().to_path_buf()).chain(std::env::split_paths(&inherited_path)),
    )?;
    // This file runs one test, so process-wide variables are set before the server or CLI start.
    unsafe {
        std::env::set_var("PATH", executable_path);
    }
    let _server = common::ServerHarness::start(None, None)?;

    fetches_once_and_reuses_the_cache_until_refreshed(&log)?;
    invalid_input_never_reaches_github(&log)?;
    refusals_are_reported_and_not_cached(&log)?;
    cached_diffs_do_not_need_gh(&log)
}

fn fetches_once_and_reuses_the_cache_until_refreshed(log: &Path) -> Result<()> {
    let fetched = remote_diff(WIDGET, WIDGET_RANGE, &[])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let html = artifact_html(&fetched)?;
    ensure!(
        html.contains("src/widget.rs"),
        "artifact omits the changed file"
    );
    ensure!(
        html.contains("example-org/widget v1.0.0...v1.1.0"),
        "artifact omits the remote label"
    );
    ensure!(
        requests(log)? == [WIDGET_REQUEST],
        "first render must fetch once"
    );

    remote_diff(
        "git@github.com:Example-Org/Widget.git",
        WIDGET_RANGE,
        &["-n", "widget upgrade"],
    )
    .assert()
    .success()
    .stdout(contains("file://"));
    ensure!(
        requests(log)?.len() == 1,
        "an equivalent origin must reuse the cached diff"
    );

    remote_diff(
        "ssh://git@github.com/example-org/widget",
        WIDGET_RANGE,
        &["--refresh"],
    )
    .assert()
    .success();
    ensure!(
        requests(log)? == [WIDGET_REQUEST, WIDGET_REQUEST],
        "--refresh must fetch again"
    );
    Ok(())
}

fn invalid_input_never_reaches_github(log: &Path) -> Result<()> {
    let requests_before = requests(log)?.len();

    remote_diff(WIDGET, "v1.0.0..v1.1.0", &[])
        .assert()
        .code(2)
        .stderr(contains("BASE...HEAD"));
    remote_diff("https://gitlab.com/example-org/widget", WIDGET_RANGE, &[])
        .assert()
        .code(2)
        .stderr(contains("github.com repository"));

    ensure!(
        requests(log)?.len() == requests_before,
        "invalid input must not reach GitHub"
    );
    Ok(())
}

fn refusals_are_reported_and_not_cached(log: &Path) -> Result<()> {
    let requests_before = requests(log)?.len();

    for _ in 0..2 {
        remote_diff("https://github.com/example-org/missing", WIDGET_RANGE, &[])
            .assert()
            .failure()
            .stderr(contains("GitHub found no such repository"));
    }

    ensure!(
        requests(log)?.len() == requests_before + 2,
        "failed fetches must not be cached"
    );
    Ok(())
}

fn cached_diffs_do_not_need_gh(log: &Path) -> Result<()> {
    let requests_before = requests(log)?.len();
    let without_gh = tempfile::tempdir()?;

    remote_diff(WIDGET, "v1.0.0...v2.0.0", &[])
        .env("PATH", without_gh.path())
        .assert()
        .code(3)
        .stderr(contains("`gh` is not on PATH"));
    remote_diff(WIDGET, WIDGET_RANGE, &[])
        .env("PATH", without_gh.path())
        .assert()
        .success();

    ensure!(
        requests(log)?.len() == requests_before,
        "a cached diff must not need gh"
    );
    Ok(())
}
