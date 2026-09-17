use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};

use super::{
    IsolatedEnv, ManagedChild, POLL_INTERVAL, READY_TIMEOUT, Sandbox, WINDOW_TITLE_PATTERN,
    command_checked, create_native_fixture, find_window, output, release_binary, retry,
    retry_value, start_server,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowState {
    Visible,
    Minimized,
    Hidden,
}

pub(super) fn run(sandbox: &Sandbox, env: &IsolatedEnv) -> Result<()> {
    let viewer = release_binary("gtl-viewer")?;
    let configuration = sandbox.config.join("git-tools.toml");
    fs::write(&configuration, "focus_window_on_diff = false\n")?;
    let _server = start_server(
        sandbox,
        env,
        &sandbox.native_data,
        "native gtl-server",
        "native-server.log",
    )?;
    let _viewer = ManagedChild::spawn(
        "native viewer",
        viewer.to_string_lossy().as_ref(),
        &["--background-diff"],
        env,
        &sandbox.root,
        &sandbox.logs.join("native-viewer.log"),
    )?;
    retry("background viewer readiness", READY_TIMEOUT, || {
        output(
            env,
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.freedesktop.DBus",
                "--object-path",
                "/org/freedesktop/DBus",
                "--method",
                "org.freedesktop.DBus.NameHasOwner",
                "dev.gittools.viewer.SingleInstance",
            ],
            &sandbox.root,
        )
        .is_ok_and(|result| {
            result.status.success() && String::from_utf8_lossy(&result.stdout).contains("true")
        })
    })?;
    assert_background(env, None)?;
    command_checked(env, viewer.to_string_lossy().as_ref(), &[], &sandbox.root)?;
    let window = retry_value("manual launch map", READY_TIMEOUT, || {
        find_window(env, WINDOW_TITLE_PATTERN).ok()
    })?;
    wait_for_focus(env, &window)?;

    let _peer = ManagedChild::spawn(
        "focus peer",
        "xmessage",
        &["-title", "GTL focus peer", "Native focus test"],
        env,
        &sandbox.root,
        &sandbox.logs.join("focus-peer.log"),
    )?;
    let peer = retry_value("focus peer map", READY_TIMEOUT, || {
        find_window(env, "^GTL focus peer$").ok()
    })?;
    let repository = create_native_fixture(sandbox, env)?;
    for enabled in [true, false, true] {
        fs::write(
            &configuration,
            format!("focus_window_on_diff = {enabled}\n"),
        )?;
        for state in [
            WindowState::Visible,
            WindowState::Minimized,
            WindowState::Hidden,
        ] {
            check_window_state(sandbox, env, &repository, &window, &peer, enabled, state)?;
        }
    }
    fs::remove_file(configuration)?;
    Ok(())
}

fn check_window_state(
    sandbox: &Sandbox,
    env: &IsolatedEnv,
    repository: &Path,
    window: &str,
    peer: &str,
    enabled: bool,
    state: WindowState,
) -> Result<()> {
    command_checked(
        env,
        sandbox.viewer_binary.to_string_lossy().as_ref(),
        &[],
        &sandbox.root,
    )?;
    wait_for_focus(env, window)?;
    match state {
        WindowState::Minimized => command_checked(
            env,
            "xdotool",
            &["windowminimize", "--sync", window],
            &sandbox.root,
        )?,
        WindowState::Hidden => {
            command_checked(env, "xdotool", &["key", "alt+F4"], &sandbox.root)?;
        }
        WindowState::Visible => {}
    }
    if state == WindowState::Hidden {
        retry("close-to-hide", READY_TIMEOUT, || {
            find_window(env, WINDOW_TITLE_PATTERN).is_err()
        })?;
    }
    command_checked(
        env,
        "xdotool",
        &["windowactivate", "--sync", peer],
        &sandbox.root,
    )?;
    for _ in 0..2 {
        command_checked(
            env,
            sandbox.cli_binary.to_string_lossy().as_ref(),
            &["diff", "--name", "native focus"],
            repository,
        )?;
        if enabled {
            wait_for_focus(env, window)?;
            command_checked(
                env,
                "xdotool",
                &["windowactivate", "--sync", peer],
                &sandbox.root,
            )?;
        } else {
            assert_background(env, Some((peer, window, state)))?;
        }
    }
    Ok(())
}

fn wait_for_focus(env: &IsolatedEnv, window: &str) -> Result<()> {
    retry("native window focus", READY_TIMEOUT, || {
        output(env, "xdotool", &["getactivewindow"], Path::new(".")).is_ok_and(|result| {
            result.status.success() && String::from_utf8_lossy(&result.stdout).trim() == window
        })
    })?;
    thread::sleep(Duration::from_millis(250));
    Ok(())
}

fn assert_background(env: &IsolatedEnv, state: Option<(&str, &str, WindowState)>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if let Some((peer, window, state)) = state {
            let active = output(env, "xdotool", &["getactivewindow"], Path::new("."))?;
            ensure!(
                active.status.success() && String::from_utf8_lossy(&active.stdout).trim() == peer,
                "background diff stole native focus from peer ({state:?})"
            );
            let properties = output(env, "xprop", &["-id", window, "WM_STATE"], Path::new("."))?;
            if state == WindowState::Minimized {
                ensure!(
                    String::from_utf8_lossy(&properties.stdout).contains("Iconic"),
                    "background diff restored minimized window"
                );
            }
            if state == WindowState::Visible {
                ensure!(
                    find_window(env, WINDOW_TITLE_PATTERN).is_ok(),
                    "background diff hid visible window"
                );
            }
            if state == WindowState::Hidden {
                ensure!(
                    find_window(env, WINDOW_TITLE_PATTERN).is_err(),
                    "background diff showed hidden window"
                );
            }
        } else {
            ensure!(
                find_window(env, WINDOW_TITLE_PATTERN).is_err(),
                "background startup showed the window"
            );
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}
