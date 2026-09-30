//! Tauri shell for the server-owned git-tools viewer.

use viewer_ipc::{viewer_get_file_filters, viewer_set_file_filters};
mod project_picker;
mod theme_icons;
mod tray_labels;
mod viewer_ipc;
mod window_activation;
mod window_controls;
#[cfg(target_os = "linux")]
mod window_frame;
mod window_launch;

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
    ViewerIpcState, viewer_activate_tab, viewer_clear_commit_selection, viewer_close_other_tabs,
    viewer_close_tab, viewer_connect, viewer_create_push, viewer_discover_project_repositories,
    viewer_edit_settings, viewer_find_diff, viewer_get_history_copy, viewer_get_project_status,
    viewer_get_push, viewer_get_push_availability, viewer_get_settings,
    viewer_get_settings_recovery, viewer_get_shell, viewer_import_project_repositories,
    viewer_list_commits, viewer_list_history, viewer_list_projects, viewer_move_tab,
    viewer_open_commit, viewer_open_diff_file, viewer_open_history, viewer_open_project,
    viewer_open_unpushed_project_diffs, viewer_read_diff_text, viewer_read_settings_file,
    viewer_refresh_tab, viewer_rename_snapshot, viewer_reset_settings, viewer_search_commits,
    viewer_search_files, viewer_select_commit, viewer_set_changes_since, viewer_set_modified_files,
    viewer_set_preference, viewer_set_project_status, viewer_set_tab_live, viewer_set_tab_pinned,
    viewer_start_push, viewer_stream_rows_cancel, viewer_stream_rows_next_batch,
    viewer_stream_rows_start, viewer_update_project, viewer_update_tab, viewer_watch_cancel,
    viewer_watch_next_batch, viewer_watch_start,
};
use window_launch::WindowLaunch;

/// Names the viewer in its window title and tray tooltip; a product name reads the same in every
/// language, so neither changes after the viewer learns its language.
const PRODUCT_NAME: &str = "git-tools";
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
    #[cfg(target_os = "linux")]
    restore_window_step(
        window_activation::remap_unfocused_wayland_window(window),
        "remap Wayland window",
    );
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

fn main_window_url(focus_diff: bool) -> WebviewUrl {
    let entrypoint = if focus_diff { "diffs" } else { "index.html" };
    WebviewUrl::App(entrypoint.into())
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
fn xlib_window_id(handle: raw_window_handle::XlibWindowHandle) -> u64 {
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
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle};
        if matches!(app.display_handle()?.as_raw(), RawDisplayHandle::Wayland(_)) {
            // GTK 3 uses the GLib program name for the Wayland window's application id.
            gtk::glib::set_prgname(Some(&app.config().identifier));
        }
    }
    let launch = WindowLaunch::from_arguments(std::env::args_os());
    let window = WebviewWindowBuilder::new(app, "main", main_window_url(launch.opens_diff()))
        .title(PRODUCT_NAME)
        .decorations(!cfg!(target_os = "windows"))
        .resizable(true)
        .shadow(true)
        .visible(false)
        .inner_size(MAIN_WINDOW_SIZE.0, MAIN_WINDOW_SIZE.1)
        .min_inner_size(MAIN_WINDOW_MIN_SIZE.0, MAIN_WINDOW_MIN_SIZE.1)
        .on_navigation(viewer_navigation_allowed)
        .build()?;
    #[cfg(target_os = "linux")]
    window_frame::configure(&window)?;
    if launch.focuses_window() {
        focus_main(&window);
    }
    // The viewer replaces these labels with localized ones through `desktop_tray_labels`.
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    app.manage(tray_labels::TrayMenuItems { show, quit });
    // The viewer recolors this icon through `desktop_theme_icons` once its theme loads.
    let builder = TrayIconBuilder::with_id(theme_icons::TRAY_ID)
        .icon(theme_icons::DEFAULT_TRAY_ICON)
        .menu(&menu)
        .tooltip(PRODUCT_NAME)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    focus_main(&window);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    builder.build(app)?;
    Ok(())
}

/// Returns the application context that `build.rs` generates.
// Clippy lints `include!`d code as local source, so it would reject Tauri's generated context.
#[allow(clippy::all, clippy::pedantic, clippy::restriction)]
fn tauri_context() -> tauri::Context<tauri::Wry> {
    tauri::tauri_build_context!()
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
            window_controls::desktop_window_state,
            window_controls::desktop_window_scale,
            window_controls::desktop_window_action,
            viewer_connect,
            project_picker::desktop_pick_project_folder,
            tray_labels::command::desktop_tray_labels,
            theme_icons::command::desktop_theme_icons,
            viewer_create_push,
            viewer_get_push,
            viewer_get_push_availability,
            viewer_start_push,
            viewer_get_shell,
            viewer_list_projects,
            viewer_discover_project_repositories,
            viewer_import_project_repositories,
            viewer_get_project_status,
            viewer_open_project,
            viewer_open_unpushed_project_diffs,
            viewer_update_project,
            viewer_set_project_status,
            viewer_activate_tab,
            viewer_move_tab,
            viewer_close_tab,
            viewer_refresh_tab,
            viewer_set_changes_since,
            viewer_update_tab,
            viewer_set_tab_live,
            viewer_rename_snapshot,
            viewer_select_commit,
            viewer_clear_commit_selection,
            viewer_set_modified_files,
            viewer_set_tab_pinned,
            viewer_close_other_tabs,
            viewer_set_preference,
            viewer_search_commits,
            viewer_open_commit,
            viewer_list_commits,
            viewer_search_files,
            viewer_find_diff,
            viewer_read_diff_text,
            viewer_list_history,
            viewer_open_history,
            viewer_get_history_copy,
            viewer_get_settings_recovery,
            viewer_reset_settings,
            viewer_get_settings,
            viewer_edit_settings,
            viewer_get_file_filters,
            viewer_set_file_filters,
            viewer_open_diff_file,
            viewer_read_settings_file,
            viewer_stream_rows_start,
            viewer_stream_rows_next_batch,
            viewer_stream_rows_cancel,
            viewer_watch_start,
            viewer_watch_next_batch,
            viewer_watch_cancel,
        ])
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if WindowLaunch::from_arguments(argv).focuses_window()
                && let Some(window) = app.get_webview_window("main")
            {
                focus_main(&window);
            }
        }))
        .setup(setup_viewer)
        .on_window_event(handle_window_event)
        .run(tauri_context())?;
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
        assert_eq!(PRODUCT_NAME, "git-tools");
        assert_eq!(MAIN_WINDOW_SIZE, (1200.0, 800.0));
        assert_eq!(MAIN_WINDOW_MIN_SIZE, (390.0, 480.0));
        assert!(matches!(
            main_window_url(false),
            WebviewUrl::App(path) if path == std::path::Path::new("index.html")
        ));
    }

    #[test]
    fn diff_launch_carries_explicit_navigation_intent() {
        assert!(matches!(
            main_window_url(true),
            WebviewUrl::App(path) if path == std::path::Path::new("diffs")
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
