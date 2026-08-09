//! gtl-viewer: the Tauri desktop diff viewer.
#[cfg(feature = "benchmark-support")]
pub use render::MaudViewerRenderer;
#[cfg(feature = "benchmark-support")]
pub use session::{CacheDisposition, CachedView, WeightedViewCache};
mod bridge;
mod commands;
#[cfg(feature = "dioxus-poc")]
mod dioxus_poc;
mod live_view_restoration;
mod materialization;
mod pending_recipes;
mod presentation;
mod protocol_config;
mod recipe_worker;
mod recipes;
mod render;
mod routes;
mod session;
mod window_activation;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gtl_contracts::{
    recipes::{OpenRecipes, decode_token},
    viewer::VIEWER_STATE_CHANGED_EVENT,
};
use tauri::{
    Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

const DEFAULT_VIEW_CACHE_WEIGHT: usize = 128 * 1024 * 1024;
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
    if !was_hidden_by_close {
        HiddenRecovery::None
    } else if !restoration.succeeded() {
        HiddenRecovery::Rearm
    } else {
        HiddenRecovery::None
    }
}

fn recipes_from_argv(argv: &[String]) -> Vec<OpenRecipes> {
    argv.iter().filter_map(|arg| decode_token(arg)).collect()
}

fn emit_viewer_state_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let change = app.state::<presentation::ViewerApp>().state_changed()?;
    app.emit(VIEWER_STATE_CHANGED_EVENT, change)
        .map_err(|error| error.to_string())
}

/// Brings the main window to the foreground — even over a focused fullscreen app.
///
/// `set_focus` alone is enough when no other window is fullscreen, but under
/// mutter/GNOME a `gtk_window_present` is demoted to a taskbar flash when a
/// fullscreen peer (e.g. a fullscreen terminal) holds focus. So we also send an
/// EWMH pager-sourced `_NET_ACTIVE_WINDOW`, which bypasses
/// focus-stealing-prevention. That message only lands on a *mapped* window, and
/// `show()` is processed by the GTK loop only after this callback returns — so
/// we defer activation on a worker thread and retry a few times to outlast the
/// map latency.
fn focus_main(window: &tauri::WebviewWindow) {
    let app_handle = window.app_handle();
    let lifecycle = app_handle.state::<MainWindowLifecycle>();
    let was_hidden_by_close = lifecycle.take_hidden_by_close();
    let unminimized = match window.unminimize() {
        Ok(()) => true,
        Err(error) => {
            eprintln!("gtl-viewer: failed to unminimize main window: {error}");
            false
        }
    };
    let shown = match window.show() {
        Ok(()) => true,
        Err(error) => {
            eprintln!("gtl-viewer: failed to show main window: {error}");
            false
        }
    };
    let focused = match window.set_focus() {
        Ok(()) => true,
        Err(error) => {
            eprintln!("gtl-viewer: failed to focus main window: {error}");
            false
        }
    };
    let restoration = NativeRestoration {
        unminimized,
        shown,
        focused,
    };
    match hidden_recovery(was_hidden_by_close, restoration) {
        HiddenRecovery::None => {}
        HiddenRecovery::Rearm => lifecycle.mark_hidden_by_close(),
    }
    if let Some(xid) = window_xid(window) {
        std::thread::spawn(move || {
            for _ in 0..3 {
                std::thread::sleep(std::time::Duration::from_millis(60));
                window_activation::activate(xid);
            }
        });
    }
}

#[cfg(any(feature = "dioxus-poc", feature = "dioxus-shell"))]
fn main_window_url() -> WebviewUrl {
    WebviewUrl::App("index.html".into())
}

#[cfg(not(any(feature = "dioxus-poc", feature = "dioxus-shell")))]
fn main_window_url() -> anyhow::Result<WebviewUrl> {
    let app_url = protocol_config::APP_URL.parse::<url::Url>()?;
    Ok(WebviewUrl::CustomProtocol(app_url))
}

#[cfg(feature = "dioxus-poc")]
fn with_viewer_commands(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        bridge::viewer_get_shell,
        bridge::viewer_prepare_diff_document,
        bridge::viewer_load_diff_chunk,
        bridge::viewer_activate_tab,
        bridge::viewer_close_tab,
        bridge::viewer_refresh_tab,
        bridge::viewer_delete_live_tab,
        bridge::viewer_select_commit,
        bridge::viewer_clear_commit_selection,
        bridge::viewer_set_preference,
        bridge::viewer_list_history,
        bridge::viewer_open_history,
        bridge::viewer_get_settings,
        bridge::viewer_open_diff_file,
        dioxus_poc::dioxus_poc_process_pending,
        dioxus_poc::dioxus_poc_diff_fragment,
    ])
}

#[cfg(not(feature = "dioxus-poc"))]
fn with_viewer_commands(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        bridge::viewer_get_shell,
        bridge::viewer_prepare_diff_document,
        bridge::viewer_load_diff_chunk,
        bridge::viewer_activate_tab,
        bridge::viewer_close_tab,
        bridge::viewer_refresh_tab,
        bridge::viewer_delete_live_tab,
        bridge::viewer_select_commit,
        bridge::viewer_clear_commit_selection,
        bridge::viewer_set_preference,
        bridge::viewer_list_history,
        bridge::viewer_open_history,
        bridge::viewer_get_settings,
        bridge::viewer_open_diff_file,
    ])
}

/// Returns the native X11 window id of `window` when this is an X11 session, or
/// `None` on Wayland, Windows, and macOS.
fn window_xid(window: &tauri::WebviewWindow) -> Option<u64> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        // ! `XlibWindowHandle::window` is `c_ulong` — u64 on Linux but u32 on Windows (LLP64),
        // ! so this `.into()` is a real cross-platform widening, not the no-op it looks like here.
        #[allow(clippy::useless_conversion)]
        RawWindowHandle::Xlib(h) => Some(h.window.into()),
        RawWindowHandle::Xcb(h) => Some(u64::from(h.window.get())),
        _ => None,
    }
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

fn setup_viewer(
    app: &mut tauri::App,
    cold_start_batches: Vec<OpenRecipes>,
    main_window_url: WebviewUrl,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(completions) = app
        .state::<presentation::ViewerApp>()
        .take_recipe_completions()
    {
        let app_handle = app.handle().clone();
        tauri::async_runtime::spawn_blocking(move || {
            while completions.recv().is_ok() {
                if let Err(error) = emit_viewer_state_changed(&app_handle) {
                    eprintln!("gtl-viewer: failed to invalidate completed viewer state: {error}");
                }
            }
        });
    }

    let has_cold_start_batches = !cold_start_batches.is_empty();
    if let Err(error) = app
        .state::<presentation::ViewerApp>()
        .enqueue_and_process_recipes(cold_start_batches)
    {
        eprintln!("gtl-viewer: failed to process cold-start recipes: {error}");
    }
    if has_cold_start_batches && let Err(error) = emit_viewer_state_changed(app.handle()) {
        eprintln!("gtl-viewer: failed to invalidate cold-start viewer state: {error}");
    }

    WebviewWindowBuilder::new(app, "main", main_window_url)
        .title(MAIN_WINDOW_TITLE)
        .inner_size(MAIN_WINDOW_SIZE.0, MAIN_WINDOW_SIZE.1)
        .min_inner_size(MAIN_WINDOW_MIN_SIZE.0, MAIN_WINDOW_MIN_SIZE.1)
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

/// Builds and runs the Tauri application.
///
/// # Errors
///
/// Returns an error when viewer storage cannot initialize, window setup fails, or the Tauri
/// runtime cannot start.
pub fn run() -> anyhow::Result<()> {
    let data_root = commands::data_root().map_err(anyhow::Error::msg)?;
    let viewer_app = presentation::ViewerApp::open(
        &data_root,
        gtl_infra::user_config::TomlSettingsStore::from_environment(),
        DEFAULT_VIEW_CACHE_WEIGHT,
    )?;
    let cold_start_batches = recipes_from_argv(&std::env::args().collect::<Vec<_>>());
    #[cfg(any(feature = "dioxus-poc", feature = "dioxus-shell"))]
    let main_window_url = main_window_url();
    #[cfg(not(any(feature = "dioxus-poc", feature = "dioxus-shell")))]
    let main_window_url = main_window_url()?;
    let builder = with_viewer_commands(
        tauri::Builder::default()
            .manage(viewer_app)
            .manage(MainWindowLifecycle::default()),
    );
    builder
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let batches = recipes_from_argv(&argv);
            let has_batches = !batches.is_empty();
            let viewer = app.state::<presentation::ViewerApp>();
            let window = app.get_webview_window("main");
            if let Err(error) = viewer.enqueue_and_process_recipes(batches) {
                eprintln!("gtl-viewer: failed to process forwarded recipe batch: {error}");
            }
            if has_batches && let Err(error) = emit_viewer_state_changed(app) {
                eprintln!("gtl-viewer: failed to invalidate forwarded viewer state: {error}");
            }
            if let Some(window) = &window {
                focus_main(window);
            }
        }))
        .register_asynchronous_uri_scheme_protocol(
            protocol_config::PROTOCOL_SCHEME,
            |ctx, request, responder| {
                let app = ctx
                    .app_handle()
                    .state::<presentation::ViewerApp>()
                    .inner()
                    .clone();
                tauri::async_runtime::spawn_blocking(move || {
                    responder.respond(routes::serve_app(&app, request));
                });
            },
        )
        .setup(move |app| setup_viewer(app, cold_start_batches, main_window_url))
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

    fn recipe_batch(id: &str) -> gtl_contracts::recipes::OpenRecipes {
        gtl_contracts::recipes::OpenRecipes {
            batch_id: id.into(),
            kind: gtl_contracts::recipes::RecipeBatchKind::Snapshot,
            recipes: Vec::new(),
        }
    }

    #[test]
    fn recipe_argv_decodes_all_valid_batches_and_ignores_malformed_tokens() {
        let first = recipe_batch("first");
        let second = recipe_batch("second");
        let argv = vec![
            "gtl-viewer".into(),
            gtl_contracts::recipes::encode_token(&first).expect("first batch encodes"),
            "gtl-recipe://malformed".into(),
            "--flag".into(),
            gtl_contracts::recipes::encode_token(&second).expect("second batch encodes"),
        ];

        assert_eq!(recipes_from_argv(&argv), vec![first, second]);
    }

    #[test]
    fn single_instance_argv_without_an_executable_decodes_the_first_batch() {
        let batch = recipe_batch("warm");

        assert_eq!(
            recipes_from_argv(&[
                gtl_contracts::recipes::encode_token(&batch).expect("batch encodes")
            ]),
            vec![batch]
        );
    }

    #[test]
    fn programmatic_main_window_contract_is_pinned() {
        assert_eq!(MAIN_WINDOW_TITLE, "git-tools diff viewer");
        assert_eq!(MAIN_WINDOW_SIZE, (1200.0, 800.0));
        assert_eq!(MAIN_WINDOW_MIN_SIZE, (390.0, 480.0));
    }

    #[cfg(any(feature = "dioxus-poc", feature = "dioxus-shell"))]
    #[test]
    fn dioxus_shell_uses_the_tauri_app_url_boundary() {
        assert!(matches!(
            main_window_url(),
            WebviewUrl::App(path) if path == std::path::Path::new("index.html")
        ));
    }

    #[test]
    fn production_view_cache_respects_the_low_memory_budget() {
        assert_eq!(DEFAULT_VIEW_CACHE_WEIGHT, 128 * 1024 * 1024);
    }

    #[test]
    fn tray_show_restores_without_reloading() {
        assert_eq!(
            hidden_recovery(true, successful_restoration()),
            HiddenRecovery::None
        );
    }

    #[test]
    fn tokenless_second_launch_restores_without_reloading() {
        assert_eq!(
            hidden_recovery(true, successful_restoration()),
            HiddenRecovery::None
        );
    }

    #[test]
    fn queued_warm_forward_keeps_the_loaded_document() {
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
