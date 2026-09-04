//! Tauri shell for the server-owned git-tools viewer.

mod viewer_ipc;
mod window_activation;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::{
    Manager, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use viewer_ipc::{
    ViewerIpcState, viewer_activate_tab, viewer_clear_commit_selection, viewer_close_tab,
    viewer_connect, viewer_delete_live_tab, viewer_edit_settings, viewer_find_diff,
    viewer_get_history_copy, viewer_get_settings, viewer_get_shell, viewer_list_commits,
    viewer_list_history, viewer_move_tab, viewer_open_diff_file, viewer_open_history,
    viewer_refresh_tab, viewer_search_files, viewer_select_commit, viewer_set_preference,
    viewer_stream_rows_cancel, viewer_stream_rows_next_batch, viewer_stream_rows_start,
    viewer_watch_cancel, viewer_watch_next_batch, viewer_watch_start,
};

const MAIN_WINDOW_TITLE: &str = "git-tools diff viewer";
const MAIN_WINDOW_SIZE: (f64, f64) = (1200.0, 800.0);
const MAIN_WINDOW_MIN_SIZE: (f64, f64) = (390.0, 480.0);

#[derive(Clone, Default)]
struct MainWindowLifecycle {
    hidden_by_close: Arc<AtomicBool>,
}

impl MainWindowLifecycle {
    fn mark_hidden_by_close(&self) {
        self.hidden_by_close.store(true, Ordering::Release);
    }

    fn take_hidden_by_close(&self) -> bool {
        self.hidden_by_close.swap(false, Ordering::AcqRel)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeRestoration {
    unminimized: bool,
    shown: bool,
    focused: bool,
}

impl NativeRestoration {
    const fn succeeded(self) -> bool {
        self.unminimized && self.shown && self.focused
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiddenRecovery {
    None,
    Rearm,
}

const fn hidden_recovery(
    was_hidden_by_close: bool,
    restoration: NativeRestoration,
) -> HiddenRecovery {
    if was_hidden_by_close && !restoration.succeeded() {
        HiddenRecovery::Rearm
    } else {
        HiddenRecovery::None
    }
}

/// Brings the main window to the foreground, including over a focused fullscreen app.
fn focus_main(window: &tauri::WebviewWindow) {
    let app_handle = window.app_handle();
    let lifecycle = app_handle.state::<MainWindowLifecycle>();
    let was_hidden_by_close = lifecycle.take_hidden_by_close();
    let unminimized = restore_window_step(window.unminimize(), "unminimize");
    let shown = restore_window_step(window.show(), "show");
    let focused = restore_window_step(window.set_focus(), "focus");
    let restoration = NativeRestoration {
        unminimized,
        shown,
        focused,
    };
    if hidden_recovery(was_hidden_by_close, restoration) == HiddenRecovery::Rearm {
        lifecycle.mark_hidden_by_close();
    }
    if let Some(xid) = window_xid(window) {
        std::thread::spawn(move || activate_window_repeatedly(xid));
    }
}

fn activate_window_repeatedly(xid: u64) {
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(60));
        window_activation::activate(xid);
    }
}

fn restore_window_step(result: tauri::Result<()>, operation: &str) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            eprintln!("gtl-viewer: failed to {operation} main window: {error}");
            false
        }
    }
}

fn main_window_url() -> WebviewUrl {
    WebviewUrl::App("index.html".into())
}

fn viewer_navigation_allowed(url: &Url) -> bool {
    matches!(
        (url.scheme(), url.host_str(), url.port_or_known_default()),
        ("tauri", Some("localhost"), _)
            | ("http", Some("tauri.localhost"), Some(80))
            | ("http", Some("127.0.0.1"), Some(8080))
    )
}

/// Returns the native X11 window id when the viewer is running through X11.
fn window_xid(window: &tauri::WebviewWindow) -> Option<u64> {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(handle) => Some(xlib_window_id(handle)),
        RawWindowHandle::Xcb(handle) => Some(u64::from(handle.window.get())),
        _ => None,
    }
}

#[cfg(all(target_pointer_width = "64", not(target_os = "windows")))]
const fn xlib_window_id(handle: raw_window_handle::XlibWindowHandle) -> u64 {
    handle.window
}

#[cfg(any(not(target_pointer_width = "64"), target_os = "windows"))]
const fn xlib_window_id(handle: raw_window_handle::XlibWindowHandle) -> u64 {
    u64::from(handle.window)
}

fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        match window.hide() {
            Ok(()) => window
                .app_handle()
                .state::<MainWindowLifecycle>()
                .mark_hidden_by_close(),
            Err(error) => eprintln!("gtl-viewer: failed to hide main window: {error}"),
        }
    }
}

fn setup_viewer(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(app, "main", main_window_url())
        .title(MAIN_WINDOW_TITLE)
        .inner_size(MAIN_WINDOW_SIZE.0, MAIN_WINDOW_SIZE.1)
        .min_inner_size(MAIN_WINDOW_MIN_SIZE.0, MAIN_WINDOW_MIN_SIZE.1)
        .on_navigation(viewer_navigation_allowed)
        .build()?;
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("git-tools diff viewer")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    focus_main(&window);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Builds and runs the Tauri shell.
///
/// # Errors
///
/// Returns an error when the Tauri runtime cannot start.
pub fn run() -> anyhow::Result<()> {
    tauri::Builder::default()
        .manage(MainWindowLifecycle::default())
        .manage(ViewerIpcState::default())
        .invoke_handler(tauri::generate_handler![
            viewer_connect,
            viewer_get_shell,
            viewer_activate_tab,
            viewer_move_tab,
            viewer_close_tab,
            viewer_refresh_tab,
            viewer_delete_live_tab,
            viewer_select_commit,
            viewer_clear_commit_selection,
            viewer_set_preference,
            viewer_list_commits,
            viewer_search_files,
            viewer_find_diff,
            viewer_list_history,
            viewer_open_history,
            viewer_get_history_copy,
            viewer_get_settings,
            viewer_edit_settings,
            viewer_open_diff_file,
            viewer_stream_rows_start,
            viewer_stream_rows_next_batch,
            viewer_stream_rows_cancel,
            viewer_watch_start,
            viewer_watch_next_batch,
            viewer_watch_cancel,
        ])
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                focus_main(&window);
            }
        }))
        .setup(setup_viewer)
        .on_window_event(handle_window_event)
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn successful_restoration() -> NativeRestoration {
        NativeRestoration {
            unminimized: true,
            shown: true,
            focused: true,
        }
    }

    #[test]
    fn programmatic_main_window_contract_is_pinned() {
        assert_eq!(MAIN_WINDOW_TITLE, "git-tools diff viewer");
        assert_eq!(MAIN_WINDOW_SIZE, (1200.0, 800.0));
        assert_eq!(MAIN_WINDOW_MIN_SIZE, (390.0, 480.0));
        assert!(matches!(
            main_window_url(),
            WebviewUrl::App(path) if path == std::path::Path::new("index.html")
        ));
    }

    #[test]
    fn navigation_stays_within_the_packaged_or_development_viewer() {
        for allowed in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
            "http://127.0.0.1:8080/",
        ] {
            assert!(viewer_navigation_allowed(&Url::parse(allowed).unwrap()));
        }
        for rejected in [
            "https://example.com/",
            "http://127.0.0.1:4317/",
            "http://localhost:8080/",
        ] {
            assert!(!viewer_navigation_allowed(&Url::parse(rejected).unwrap()));
        }
    }

    #[test]
    fn tray_show_and_second_launch_restore_without_reloading() {
        assert_eq!(
            hidden_recovery(true, successful_restoration()),
            HiddenRecovery::None
        );
    }

    #[test]
    fn failed_native_restoration_rearms_close_hidden_recovery() {
        for restoration in [
            NativeRestoration {
                unminimized: false,
                shown: true,
                focused: true,
            },
            NativeRestoration {
                unminimized: true,
                shown: false,
                focused: true,
            },
            NativeRestoration {
                unminimized: true,
                shown: true,
                focused: false,
            },
        ] {
            assert_eq!(hidden_recovery(true, restoration), HiddenRecovery::Rearm);
        }
    }

    #[test]
    fn close_hidden_lifecycle_flag_is_one_shot_and_clone_shared() {
        let lifecycle = MainWindowLifecycle::default();
        let clone = lifecycle.clone();
        assert!(!lifecycle.take_hidden_by_close());

        lifecycle.mark_hidden_by_close();

        assert!(clone.take_hidden_by_close());
        assert!(!lifecycle.take_hidden_by_close());
    }
}
