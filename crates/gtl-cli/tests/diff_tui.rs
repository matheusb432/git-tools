#![cfg(unix)]

use std::{
    fmt::Write as _,
    os::fd::AsRawFd as _,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};
use expectrl::Expect as _;
use predicates::{prelude::PredicateBooleanExt as _, str::contains};
use unicode_width::UnicodeWidthStr as _;

mod common;

fn git(path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()?;
    ensure!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn reviews_refreshes_and_restores_the_terminal_through_the_cli() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let repo = directory.path();
    let original = prepare_repository(repo)?;
    let _server = common::ServerHarness::start(None, Some(repo))?;
    assert_invocations(repo);

    let mut command = Command::new(common::cli_binary());
    command
        .args(["diff", "tui", "HEAD", "--id", "RP"])
        .current_dir(repo)
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color");
    let mut session = expectrl::Session::spawn(command)?;
    let mut parser = vt100::Parser::new(12, 40, 0);
    session.set_expect_timeout(Some(Duration::from_secs(15)));
    session.get_process_mut().set_window_size(40, 12)?;
    wait_screen(&mut session, &mut parser, "code.rs")?;
    session.send("v")?;
    wait_screen(&mut session, &mut parser, "File marked reviewed")?;
    session.send("v")?;
    wait_screen(&mut session, &mut parser, "File marked unreviewed")?;
    session.send("]")?;
    wait_screen(&mut session, &mut parser, "changed_thirty")?;
    assert_keyword_color(&parser, "pub")?;
    click_text(&mut session, &parser, "Full")?;
    session.send("g")?;
    wait_screen(&mut session, &mut parser, "unchanged 1")?;
    session.send("/changed_thirty\r")?;
    wait_screen(&mut session, &mut parser, "Match 1/1")?;
    std::fs::write(
        repo.join("code.rs"),
        original.replace("// unchanged 30", "pub fn refreshed_thirty() -> u32 { 42 }"),
    )?;
    click_text(&mut session, &parser, "Refresh")?;
    wait_screen(&mut session, &mut parser, "refreshed_thirty")?;
    parser.screen_mut().set_size(30, 100);
    session.get_process_mut().set_window_size(100, 30)?;
    wait_screen(&mut session, &mut parser, "r Refresh")?;
    let started = Instant::now();
    session.send(format!(
        "{}{}",
        "\u{1b}[<35;75;10M".repeat(256),
        click_sequence(&parser, "notes.md")?,
    ))?;
    wait_screen(&mut session, &mut parser, "review from mouse")?;
    ensure!(
        started.elapsed() < Duration::from_secs(2),
        "mouse movement delayed the file click"
    );
    click_text(&mut session, &parser, "Filter files")?;
    session.send("code.rs")?;
    wait_screen(&mut session, &mut parser, "FILES 1/2")?;
    session.send("\r")?;
    session.send("\u{3}")?;
    session.expect("\u{1b}[?1006l")?;
    session.expect("\u{1b}[?1049l")?;
    session.expect(expectrl::Eof)?;
    assert_terminal_restored(&mut session)
}

fn prepare_repository(repo: &Path) -> Result<String> {
    git(repo, &["init", "-q", "-b", "main"])?;
    git(repo, &["config", "user.name", "Example Author"])?;
    git(repo, &["config", "user.email", "author@example.invalid"])?;
    let mut original = String::new();
    for line in 1..=50 {
        writeln!(original, "// unchanged {line}")?;
    }
    std::fs::write(repo.join("code.rs"), &original)?;
    git(repo, &["add", "."])?;
    git(
        repo,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "Initial code",
        ],
    )?;
    std::fs::write(
        repo.join("code.rs"),
        original.replace("// unchanged 30", "pub fn changed_thirty() -> u32 { 42 }"),
    )?;
    std::fs::write(repo.join("notes.md"), "# review from mouse\n")?;
    Ok(original)
}

fn assert_invocations(repo: &Path) {
    assert_cmd::Command::new(common::cli_binary())
        .args(["diff", "tui", "HEAD"])
        .current_dir(repo)
        .assert()
        .code(2)
        .stderr(contains("interactive terminal"));
    for arguments in [
        vec!["diff", "tui", "--recursive"],
        vec!["diff", "tui", "--raw"],
        vec!["diff", "tui", "HEAD", "--last"],
    ] {
        assert_cmd::Command::new(common::cli_binary())
            .args(arguments)
            .current_dir(repo)
            .assert()
            .code(2);
    }
    assert_cmd::Command::new(common::cli_binary())
        .args(["d", "tui", "--help"])
        .assert()
        .success()
        .stdout(contains("--last").and(contains("--id")));
}

fn text_position(parser: &vt100::Parser, text: &str) -> Result<(u16, u16)> {
    for (row, line) in parser.screen().contents().lines().enumerate() {
        if let Some(byte) = line.find(text) {
            return Ok((u16::try_from(line[..byte].width())?, u16::try_from(row)?));
        }
    }
    anyhow::bail!("missing {text:?}:\n{}", parser.screen().contents())
}

fn click_text(
    session: &mut expectrl::session::OsSession,
    parser: &vt100::Parser,
    text: &str,
) -> Result<()> {
    session.send(click_sequence(parser, text)?)?;
    Ok(())
}

fn click_sequence(parser: &vt100::Parser, text: &str) -> Result<String> {
    let (column, row) = text_position(parser, text)?;
    Ok(format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column + 1,
        row + 1,
        column + 1,
        row + 1
    ))
}

fn assert_keyword_color(parser: &vt100::Parser, keyword: &str) -> Result<()> {
    let (column, row) = text_position(parser, keyword)?;
    let cell = parser
        .screen()
        .cell(row, column)
        .ok_or_else(|| anyhow::anyhow!("keyword cell"))?;
    ensure!(
        cell.fgcolor() == vt100::Color::Rgb(199, 169, 255),
        "keyword is not syntax highlighted"
    );
    Ok(())
}

fn assert_terminal_restored(session: &mut expectrl::session::OsSession) -> Result<()> {
    let handle = session.get_process().get_raw_handle()?;
    let mut restored = std::mem::MaybeUninit::<libc::termios>::uninit();
    // tcgetattr initializes the complete value when it succeeds.
    ensure!(
        unsafe { libc::tcgetattr(handle.as_raw_fd(), restored.as_mut_ptr()) } == 0,
        "read restored terminal flags"
    );
    let restored = unsafe { restored.assume_init() };
    ensure!(
        restored.c_lflag & (libc::ICANON | libc::ISIG) == libc::ICANON | libc::ISIG,
        "terminal canonical input and signals were not restored"
    );
    ensure!(
        !session.get_process().get_echo()?,
        "the PTY starts with echo disabled"
    );
    ensure!(
        matches!(
            session.get_process().wait()?,
            expectrl::process::unix::WaitStatus::Exited(_, 0)
        ),
        "pager failed"
    );
    Ok(())
}

fn wait_screen(
    session: &mut expectrl::session::OsSession,
    parser: &mut vt100::Parser,
    text: &str,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffer = [0; 8192];
    loop {
        match session.try_read(&mut buffer) {
            Ok(0) => anyhow::bail!(
                "pager exited before {text:?}:\n{}",
                parser.screen().contents()
            ),
            Ok(length) => parser.process(&buffer[..length]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
        let screen = parser.screen().contents();
        if screen.contains(text) {
            println!("Screen showing {text:?}:\n{screen}");
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "pager did not show {text:?}:\n{screen}"
        );
    }
}
