//! gtl-viewer: the custom-origin htmx desktop viewer.
#[cfg(feature = "benchmark-support")]
pub use render::MaudViewerRenderer;
#[cfg(feature = "benchmark-support")]
pub use session::{CacheDisposition, CachedView, WeightedViewCache};
mod commands;
mod presentation;
mod protocol_config;
mod recipes;
mod render;
mod routes;
mod session;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use contracts::recipes::{OpenRecipes, decode_token};
use tauri::{
    Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

use crate::session::{PendingRecipes, PendingRecipesError};

const DEFAULT_VIEW_CACHE_WEIGHT: usize = 128 * 1024 * 1024;
const MAIN_WINDOW_TITLE: &str = "git-tools diff viewer";
const MAIN_WINDOW_SIZE: (f64, f64) = (1200.0, 800.0);
const MAIN_WINDOW_MIN_SIZE: (f64, f64) = (720.0, 480.0);

#[derive(Clone, Default)]
struct MainWindowLifecycle {
    hidden_by_close: Arc<AtomicBool>,
    resume_counter: Arc<AtomicU64>,
}

impl MainWindowLifecycle {
    fn mark_hidden_by_close(&self) {
        self.hidden_by_close.store(true, Ordering::Release);
    }

    fn take_hidden_by_close(&self) -> bool {
        self.hidden_by_close.swap(false, Ordering::AcqRel)
    }

    fn next_resume_nonce(&self) -> Option<routes::ResumeNonce> {
        let previous = self
            .resume_counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .ok()?;
        routes::ResumeNonce::try_new(previous.checked_add(1)?)
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
    Reload,
}

const fn hidden_recovery(
    was_hidden_by_close: bool,
    has_queued_work: bool,
    restoration: NativeRestoration,
) -> HiddenRecovery {
    if !was_hidden_by_close {
        HiddenRecovery::None
    } else if !restoration.succeeded() {
        HiddenRecovery::Rearm
    } else if has_queued_work {
        HiddenRecovery::Reload
    } else {
        HiddenRecovery::None
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ForwardRecipesError<E> {
    Queue(PendingRecipesError),
    Wake(E),
}

impl<E: std::fmt::Display> std::fmt::Display for ForwardRecipesError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queue(error) => error.fmt(formatter),
            Self::Wake(error) => write!(formatter, "failed to wake recipe viewer: {error}"),
        }
    }
}

fn recipes_from_argv(argv: &[String]) -> Vec<OpenRecipes> {
    argv.iter().filter_map(|arg| decode_token(arg)).collect()
}

fn enqueue_batches(
    pending: &PendingRecipes,
    batches: Vec<OpenRecipes>,
) -> Result<bool, PendingRecipesError> {
    let has_batches = !batches.is_empty();
    for batch in batches {
        pending.push(batch)?;
    }
    Ok(has_batches)
}

fn enqueue_and_wake<E>(
    pending: &PendingRecipes,
    batches: Vec<OpenRecipes>,
    wake: impl FnOnce() -> Result<(), E>,
) -> Result<bool, ForwardRecipesError<E>> {
    let has_queued_work = enqueue_batches(pending, batches).map_err(ForwardRecipesError::Queue)?;
    if has_queued_work {
        wake().map_err(ForwardRecipesError::Wake)?;
    }
    Ok(has_queued_work)
}

fn forward_recipes<E>(
    pending: &PendingRecipes,
    batches: Vec<OpenRecipes>,
    wake: impl FnOnce() -> Result<(), E>,
    focus: impl FnOnce(bool),
) -> Result<(), ForwardRecipesError<E>> {
    let result = enqueue_and_wake(pending, batches, wake);
    let has_queued_work = match &result {
        Ok(has_queued_work) => *has_queued_work,
        Err(ForwardRecipesError::Wake(_)) => true,
        Err(ForwardRecipesError::Queue(_)) => false,
    };
    focus(has_queued_work);
    result.map(|_| ())
}

/// Brings the main window to the foreground — even over a focused fullscreen app.
///
/// `set_focus` alone is enough when no other window is fullscreen, but under
/// mutter/GNOME a `gtk_window_present` is demoted to a taskbar flash when a
/// fullscreen peer (e.g. a fullscreen terminal) holds focus. So we also send an
/// EWMH pager-sourced `_NET_ACTIVE_WINDOW` via the PAL, which bypasses
/// focus-stealing-prevention. That message only lands on a *mapped* window, and
/// `show()` is processed by the GTK loop only after this callback returns — so
/// we defer the activation on a worker thread (the PAL opens its own X display,
/// so this is thread-safe) and retry a few times to outlast the map latency.
fn focus_main(window: &tauri::WebviewWindow, has_queued_work: bool) {
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
    match hidden_recovery(was_hidden_by_close, has_queued_work, restoration) {
        HiddenRecovery::None => {}
        HiddenRecovery::Rearm => lifecycle.mark_hidden_by_close(),
        HiddenRecovery::Reload => {
            let result = lifecycle
                .next_resume_nonce()
                .ok_or_else(|| "resume nonce exhausted".to_owned())
                .and_then(resume_url)
                .and_then(|url| window.navigate(url).map_err(|error| error.to_string()));
            if let Err(error) = result {
                eprintln!("gtl-viewer: failed to restore hidden recipe window: {error}");
            }
        }
    }
    if let Some(xid) = window_xid(window) {
        std::thread::spawn(move || {
            for _ in 0..3 {
                std::thread::sleep(std::time::Duration::from_millis(60));
                gtl_platform::activate_window(xid);
            }
        });
    }
}

fn resume_url(nonce: routes::ResumeNonce) -> Result<tauri::Url, String> {
    let mut url = protocol_config::APP_URL
        .parse::<tauri::Url>()
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("resume", &nonce.get().to_string());
    Ok(url)
}

/// Returns the native X11 window id of `window` when this is an X11 session, or
/// `None` on Wayland/Windows/macOS, where `activate_window` is a no-op anyway.
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

/// Build and run the Tauri application. Called by `main.rs`.
///
/// # Panics
/// Panics when viewer storage cannot initialize or the Tauri runtime cannot start.
pub fn run() {
    let data_root = commands::data_root().expect("viewer data root resolves");
    let viewer_app = presentation::ViewerApp::open(
        &data_root,
        infra::user_config::TomlSettingsStore::from_environment(),
        DEFAULT_VIEW_CACHE_WEIGHT,
    )
    .expect("viewer app state opens");
    let cold_start_batches = recipes_from_argv(&std::env::args().collect::<Vec<_>>());
    tauri::Builder::default()
        .manage(viewer_app)
        .manage(MainWindowLifecycle::default())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let batches = recipes_from_argv(&argv);
            let viewer = app.state::<presentation::ViewerApp>();
            let window = app.get_webview_window("main");
            if let Err(error) = forward_recipes(
                viewer.pending(),
                batches,
                || app.emit("recipes-pending", ()),
                |has_queued_work| {
                    if let Some(window) = &window {
                        focus_main(window, has_queued_work);
                    }
                },
            ) {
                eprintln!("gtl-viewer: failed to forward recipe batch: {error}");
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
        .setup(move |app| {
            if let Err(error) = enqueue_batches(
                app.state::<presentation::ViewerApp>().pending(),
                cold_start_batches,
            ) {
                eprintln!("gtl-viewer: failed to enqueue cold-start recipes: {error}");
            }
            let app_url = protocol_config::APP_URL
                .parse()
                .expect("build-validated app URL");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::CustomProtocol(app_url))
                .title(MAIN_WINDOW_TITLE)
                .inner_size(MAIN_WINDOW_SIZE.0, MAIN_WINDOW_SIZE.1)
                .min_inner_size(MAIN_WINDOW_MIN_SIZE.0, MAIN_WINDOW_MIN_SIZE.1)
                .build()?;
            // Tray: Show / Quit. Quit is the only real exit (keep-warm lifecycle).
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("git-tools diff viewer")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            focus_main(&w, false);
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
        })
        .on_window_event(handle_window_event)
        .run(tauri::generate_context!())
        .expect("error while running gtl-viewer");
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

    fn recipe_batch(id: &str) -> contracts::recipes::OpenRecipes {
        contracts::recipes::OpenRecipes {
            batch_id: id.into(),
            kind: contracts::recipes::RecipeBatchKind::Snapshot,
            recipes: Vec::new(),
        }
    }

    #[test]
    fn recipe_argv_decodes_all_valid_batches_and_ignores_malformed_tokens() {
        let first = recipe_batch("first");
        let second = recipe_batch("second");
        let argv = vec![
            "gtl-viewer".into(),
            contracts::recipes::encode_token(&first),
            "gtl-recipe://malformed".into(),
            "--flag".into(),
            contracts::recipes::encode_token(&second),
        ];

        assert_eq!(recipes_from_argv(&argv), vec![first, second]);
    }

    #[test]
    fn single_instance_argv_without_an_executable_decodes_the_first_batch() {
        let batch = recipe_batch("warm");

        assert_eq!(
            recipes_from_argv(&[contracts::recipes::encode_token(&batch)]),
            vec![batch]
        );
    }

    #[test]
    fn enqueue_happens_before_the_payload_free_wake() {
        let pending = PendingRecipes::default();
        let batch = recipe_batch("ordered");

        enqueue_and_wake(&pending, vec![batch.clone()], || {
            assert_eq!(
                pending.try_drain().expect("inspect queue"),
                vec![batch.clone()]
            );
            pending.prepend(vec![batch.clone()]).expect("restore queue");
            Ok::<(), &'static str>(())
        })
        .expect("wake succeeds");

        assert_eq!(pending.drain(), vec![batch]);
    }

    #[test]
    fn failed_wake_keeps_the_batch_for_atomic_drain() {
        let pending = PendingRecipes::default();
        let batch = recipe_batch("durable");

        let result = enqueue_and_wake(&pending, vec![batch.clone()], || Err("not listening"));

        assert!(matches!(
            result,
            Err(ForwardRecipesError::Wake("not listening"))
        ));
        assert_eq!(pending.drain(), vec![batch]);
    }

    #[test]
    fn forwarding_orders_durable_queue_then_wake_then_focus() {
        let pending = PendingRecipes::default();
        let batch = recipe_batch("ordered-forward");
        let events = std::cell::RefCell::new(Vec::new());

        forward_recipes(
            &pending,
            vec![batch.clone()],
            || {
                assert_eq!(pending.drain(), vec![batch.clone()]);
                pending.push(batch.clone()).expect("restore queued batch");
                events.borrow_mut().push("wake");
                Ok::<(), &'static str>(())
            },
            |queued| {
                events.borrow_mut().push(if queued {
                    "focus-with-work"
                } else {
                    "focus-without-work"
                });
            },
        )
        .expect("forward succeeds");

        assert_eq!(*events.borrow(), ["wake", "focus-with-work"]);
    }

    #[test]
    fn tokenless_forward_focuses_without_claiming_queued_work() {
        let pending = PendingRecipes::default();
        let mut focused_with_work = None;

        forward_recipes(
            &pending,
            Vec::new(),
            || -> Result<(), &'static str> { panic!("empty forwarding must not wake") },
            |queued| focused_with_work = Some(queued),
        )
        .expect("empty forwarding succeeds");

        assert_eq!(focused_with_work, Some(false));
        assert!(pending.drain().is_empty());
    }

    #[test]
    fn programmatic_main_window_contract_is_pinned() {
        assert_eq!(MAIN_WINDOW_TITLE, "git-tools diff viewer");
        assert_eq!(MAIN_WINDOW_SIZE, (1200.0, 800.0));
        assert_eq!(MAIN_WINDOW_MIN_SIZE, (720.0, 480.0));
    }

    #[test]
    fn production_view_cache_respects_the_low_memory_budget() {
        assert_eq!(DEFAULT_VIEW_CACHE_WEIGHT, 128 * 1024 * 1024);
    }

    #[test]
    fn tray_show_restores_without_reloading() {
        assert_eq!(
            hidden_recovery(true, false, successful_restoration()),
            HiddenRecovery::None
        );
    }

    #[test]
    fn tokenless_second_launch_restores_without_reloading() {
        assert_eq!(
            hidden_recovery(true, false, successful_restoration()),
            HiddenRecovery::None
        );
    }

    #[test]
    fn queued_warm_forward_reloads_a_restored_close_hidden_window() {
        assert_eq!(
            hidden_recovery(true, true, successful_restoration()),
            HiddenRecovery::Reload
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
            assert_eq!(
                hidden_recovery(true, true, restoration),
                HiddenRecovery::Rearm
            );
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

    #[test]
    fn resume_nonces_are_positive_monotonic_and_clone_shared() {
        let lifecycle = MainWindowLifecycle::default();
        let clone = lifecycle.clone();

        let first = lifecycle.next_resume_nonce().expect("nonce available");
        let second = clone.next_resume_nonce().expect("nonce available");

        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
    }

    #[test]
    fn resume_url_preserves_the_configured_origin_and_has_one_typed_query() {
        let nonce = routes::ResumeNonce::try_new(9).expect("positive nonce");
        let url = resume_url(nonce).expect("build-validated URL");

        assert_eq!(url.scheme(), protocol_config::PROTOCOL_SCHEME);
        assert_eq!(url.host_str(), Some(protocol_config::APP_HOST));
        assert_eq!(url.path(), "/");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![(
                std::borrow::Cow::Borrowed("resume"),
                std::borrow::Cow::Borrowed("9")
            )]
        );
    }
}
