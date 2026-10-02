#![cfg(unix)]

use std::{
    fmt::Write as _,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, ensure};
use expectrl::Expect as _;
use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthStr as _;

mod common;

const SAMPLES: usize = 7;
const MOUSE_MOVES: usize = 60;
const WHEEL_EVENTS: usize = 40;
const SEARCH_EVENTS: usize = 40;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Configuration {
    schema: u32,
    workload: String,
    runner: String,
    toolchain: String,
    machine: String,
    profile: String,
    terminal: (u16, u16),
    files: Vec<usize>,
    samples: usize,
    mouse_moves: usize,
    wheel_events: usize,
    search_events: usize,
}

#[derive(Serialize, Deserialize)]
struct Measurements {
    files: usize,
    initial_read_ms: f64,
    click_ms: Vec<f64>,
    move_then_click_ms: Vec<f64>,
    wheel_burst_ms: Vec<f64>,
    refresh_ms: Vec<f64>,
    search_ms: Vec<f64>,
    search_next_ms: Vec<f64>,
    search_burst_ms: Vec<f64>,
}

#[derive(Serialize, Deserialize)]
struct Report {
    configuration: Configuration,
    cli_binary: String,
    measurements: Vec<Measurements>,
}

#[test]
#[ignore = "release PTY benchmark; run just bench-tui --update to establish a baseline"]
fn terminal_interaction_latency() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "benchmark requires --release");
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.artifacts/benchmarks/terminal-diff");
    std::fs::create_dir_all(&directory)?;
    let configuration = Configuration {
        schema: 2,
        workload: "added-rust-files-v1; file 0 has 400 lines, others 12; warm interactions; unchanged snapshot limits".into(),
        runner: "expectrl-0.9/vt100-0.16; 1ms observation; complete frames; 80ms between samples; 20s deadline".into(),
        toolchain: command_output("rustc", &["-Vv"])?,
        machine: command_output("uname", &["-nmr"])?,
        profile: "release".into(),
        terminal: (120, 36),
        files: vec![2, 1500],
        samples: SAMPLES,
        mouse_moves: MOUSE_MOVES,
        wheel_events: WHEEL_EVENTS,
        search_events: SEARCH_EVENTS,
    };
    let update = std::env::var("GTL_TUI_BENCH_UPDATE").is_ok_and(|value| value == "--update");
    let baseline_path = directory.join("baseline.json");
    let baseline: Option<Report> = if baseline_path.exists() {
        let stored = serde_json::from_slice::<Report>(&std::fs::read(&baseline_path)?);
        if update {
            stored
                .ok()
                .filter(|baseline| baseline.configuration == configuration)
        } else {
            let baseline = stored?;
            ensure!(
                baseline.configuration == configuration,
                "incompatible terminal benchmark baseline"
            );
            Some(baseline)
        }
    } else {
        None
    };
    ensure!(
        update || baseline.is_some(),
        "missing baseline; run just bench-tui --update"
    );
    let _server = common::ServerHarness::start(None, None)?;
    let mut measurements = Vec::new();
    for &files in &configuration.files {
        measurements.push(measure(files)?);
    }
    let report = Report {
        configuration,
        cli_binary: common::cli_binary().to_string_lossy().into_owned(),
        measurements,
    };
    let bytes = serde_json::to_vec_pretty(&report)?;
    std::fs::write(directory.join("current.json"), &bytes)?;
    print_comparison(&report, baseline.as_ref());
    if update {
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        std::io::Write::write_all(&mut temporary, &bytes)?;
        temporary.persist(&baseline_path)?;
    }
    Ok(())
}

fn command_output(program: &str, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program).args(arguments).output()?;
    ensure!(output.status.success(), "{program} failed");
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn measure(files: usize) -> Result<Measurements> {
    let directory = tempfile::tempdir()?;
    prepare_repository(directory.path(), files)?;
    let mut command = Command::new(common::cli_binary());
    command
        .args(["diff", "tui", "HEAD"])
        .current_dir(directory.path())
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color");
    let started = Instant::now();
    let mut session = expectrl::Session::spawn(command)?;
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session.get_process_mut().set_window_size(120, 36)?;
    let mut parser = vt100::Parser::new(36, 120, 0);
    wait_screen(&mut session, &mut parser, "file_0000 line_0000 revision_0")?;
    let mut result = Measurements {
        files,
        initial_read_ms: started.elapsed().as_secs_f64() * 1000.0,
        click_ms: Vec::new(),
        move_then_click_ms: Vec::new(),
        wheel_burst_ms: Vec::new(),
        refresh_ms: Vec::new(),
        search_ms: Vec::new(),
        search_next_ms: Vec::new(),
        search_burst_ms: Vec::new(),
    };
    for sample in 0..SAMPLES {
        let revision = format!("file_0000 line_0000 revision_{sample}");
        let input = click(&parser, "file_0001.rs")?;
        result.click_ms.push(send_until(
            &mut session,
            &mut parser,
            &input,
            "file_0001 line_0000",
        )?);
        session.send("g")?;
        wait_screen(&mut session, &mut parser, &revision)?;
        let burst = format!(
            "{}{}",
            "\x1b[<35;75;10M".repeat(MOUSE_MOVES),
            click(&parser, "file_0001.rs")?
        );
        result.move_then_click_ms.push(send_until(
            &mut session,
            &mut parser,
            &burst,
            "file_0001 line_0000",
        )?);
        session.send("g")?;
        wait_screen(&mut session, &mut parser, &revision)?;
        let burst = format!("{}/", "\x1b[<65;75;10M".repeat(WHEEL_EVENTS));
        result
            .wheel_burst_ms
            .push(send_until(&mut session, &mut parser, &burst, "/ ▏")?);
        ensure!(
            parser.screen().contents().contains("file_0000 line_0120"),
            "wheel events were lost:\n{}",
            parser.screen().contents()
        );
        session.send("\rg")?;
        wait_screen(&mut session, &mut parser, &revision)?;
        write_file(directory.path(), 0, sample + 1)?;
        result.refresh_ms.push(send_until(
            &mut session,
            &mut parser,
            "r",
            &format!("file_0000 line_0000 revision_{}", sample + 1),
        )?);
        measure_search(&mut session, &mut parser, &mut result, sample + 1)?;
    }
    session.send("\x03")?;
    session.expect(expectrl::Eof)?;
    ensure!(
        matches!(
            session.get_process().wait()?,
            expectrl::process::unix::WaitStatus::Exited(_, 0)
        ),
        "pager failed"
    );
    Ok(result)
}

fn measure_search(
    session: &mut expectrl::session::OsSession,
    parser: &mut vt100::Parser,
    result: &mut Measurements,
    revision: usize,
) -> Result<()> {
    let matches = 400 + (result.files - 1) * 12;
    result.search_ms.push(send_until(
        session,
        parser,
        "/line_\r",
        &format!("Match 1/{matches}: line_"),
    )?);
    result.search_next_ms.push(send_until(
        session,
        parser,
        "n",
        &format!("Match 2/{matches}: line_"),
    )?);
    result.search_burst_ms.push(send_until(
        session,
        parser,
        &"n".repeat(SEARCH_EVENTS),
        &format!("Match {}/{matches}: line_", SEARCH_EVENTS + 2),
    )?);
    ensure!(
        parser.screen().contents().contains(&format!(
            "file_0000 line_{:04} revision_{revision}",
            SEARCH_EVENTS + 1,
        )),
        "search events were lost:\n{}",
        parser.screen().contents()
    );
    session.send("/\rg")?;
    wait_screen(
        session,
        parser,
        &format!("file_0000 line_0000 revision_{revision}"),
    )
}

fn prepare_repository(repo: &Path, files: usize) -> Result<()> {
    for arguments in [
        vec!["init", "-q", "-b", "main"],
        vec![
            "-c",
            "user.name=Benchmark",
            "-c",
            "user.email=benchmark@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "baseline",
        ],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(arguments)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()?;
        ensure!(
            output.status.success(),
            "git: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for file in 0..files {
        write_file(repo, file, 0)?;
    }
    Ok(())
}

fn write_file(repo: &Path, file: usize, revision: usize) -> Result<()> {
    let mut source = String::new();
    for line in 0..if file == 0 { 400 } else { 12 } {
        writeln!(
            source,
            "// file_{file:04} line_{line:04} revision_{revision}"
        )?;
    }
    source.push_str("pub fn answer() -> u32 { 42 }\n");
    std::fs::write(repo.join(format!("file_{file:04}.rs")), source)?;
    Ok(())
}

fn click(parser: &vt100::Parser, text: &str) -> Result<String> {
    for (row, line) in parser.screen().contents().lines().enumerate() {
        if let Some(byte) = line.find(text) {
            let column = line[..byte].width() + 1;
            let row = row + 1;
            return Ok(format!("\x1b[<0;{column};{row}M\x1b[<0;{column};{row}m"));
        }
    }
    anyhow::bail!("missing file {text}:\n{}", parser.screen().contents())
}

fn send_until(
    session: &mut expectrl::session::OsSession,
    parser: &mut vt100::Parser,
    input: &str,
    text: &str,
) -> Result<f64> {
    std::thread::sleep(Duration::from_millis(80));
    let started = Instant::now();
    session.send(input)?;
    wait_screen(session, parser, text)?;
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

fn wait_screen(
    session: &mut expectrl::session::OsSession,
    parser: &mut vt100::Parser,
    text: &str,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut buffer = vec![0; 65536];
    let mut tail = Vec::new();
    loop {
        match session.try_read(&mut buffer) {
            Ok(0) => anyhow::bail!("pager exited before {text}"),
            Ok(length) => {
                parser.process(&buffer[..length]);
                tail.extend_from_slice(&buffer[..length]);
                tail.drain(..tail.len().saturating_sub(6));
                if tail == b"\x1b[?25l" && parser.screen().contents().contains(text) {
                    return Ok(());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error).context("read terminal"),
        }
        ensure!(
            Instant::now() < deadline,
            "missing {text}:\n{}",
            parser.screen().contents()
        );
    }
}

fn print_comparison(report: &Report, baseline: Option<&Report>) {
    for (index, measurement) in report.measurements.iter().enumerate() {
        let previous = baseline.and_then(|baseline| baseline.measurements.get(index));
        for (name, samples, before) in [
            (
                "click",
                &measurement.click_ms,
                previous.map(|value| &value.click_ms),
            ),
            (
                "move then click",
                &measurement.move_then_click_ms,
                previous.map(|value| &value.move_then_click_ms),
            ),
            (
                "wheel burst",
                &measurement.wheel_burst_ms,
                previous.map(|value| &value.wheel_burst_ms),
            ),
            (
                "refresh",
                &measurement.refresh_ms,
                previous.map(|value| &value.refresh_ms),
            ),
            (
                "search",
                &measurement.search_ms,
                previous.map(|value| &value.search_ms),
            ),
            (
                "next match",
                &measurement.search_next_ms,
                previous.map(|value| &value.search_next_ms),
            ),
            (
                "search burst",
                &measurement.search_burst_ms,
                previous.map(|value| &value.search_burst_ms),
            ),
        ] {
            let mut ordered = samples.clone();
            ordered.sort_by(f64::total_cmp);
            let median = ordered[ordered.len() / 2];
            println!(
                "{} files, {name}: median {median:.2}ms, max {:.2}ms{}",
                measurement.files,
                ordered[ordered.len() - 1],
                before.map_or_else(String::new, |before| {
                    let mut ordered = before.clone();
                    ordered.sort_by(f64::total_cmp);
                    format!(", baseline median {:.2}ms", ordered[ordered.len() / 2])
                })
            );
        }
    }
}
