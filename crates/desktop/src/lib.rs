//! gtl-viewer: the Tauri diff viewer. Hosts the store's self-contained HTML
//! artifacts as browser-style tabs over a scoped `diff://` scheme (ADR-0004).
mod diffs;
mod history;
mod protocol;

use diffs::{diff_ref_from_argv, PendingDiffs};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};
use tauri::http::{Response, StatusCode};

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
fn focus_main(window: &tauri::WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    if let Some(xid) = window_xid(window) {
        std::thread::spawn(move || {
            for _ in 0..3 {
                std::thread::sleep(std::time::Duration::from_millis(60));
                gtl_platform::activate_window(xid);
            }
        });
    }
}

/// Returns the native X11 window id of `window` when this is an X11 session, or
/// `None` on Wayland/Windows/macOS, where `activate_window` is a no-op anyway.
fn window_xid(window: &tauri::WebviewWindow) -> Option<u64> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(h) => Some(h.window),
        RawWindowHandle::Xcb(h) => Some(h.window.get() as u64),
        _ => None,
    }
}

/// Frontend pulls queued diff refs on mount (cold-start + any that arrived first).
#[tauri::command]
fn drain_pending_diffs(state: tauri::State<'_, PendingDiffs>) -> Vec<String> {
    state.drain()
}

/// Return all stored diff previews sorted newest-first, for the history panel.
#[tauri::command]
fn list_history() -> Vec<history::HistoryEntry> {
    match protocol::store_root() {
        Some(root) => history::entries_from_store(&root),
        None => Vec::new(),
    }
}

/// Build and run the Tauri application. Called by `main.rs`.
pub fn run() {
    tauri::Builder::default()
        .manage(PendingDiffs::default())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Second launch: queue its diff, raise the window, tell the frontend.
            if let Some(diff_ref) = diff_ref_from_argv(&argv) {
                app.state::<PendingDiffs>().push(diff_ref.clone());
                let _ = app.emit("open-diff", diff_ref);
            }
            if let Some(win) = app.get_webview_window("main") {
                focus_main(&win);
            }
        }))
        .invoke_handler(tauri::generate_handler![drain_pending_diffs, list_history])
        .register_asynchronous_uri_scheme_protocol("diff", |_ctx, request, responder| {
            responder.respond(serve_diff(&request));
        })
        .setup(|app| {
            // Cold-start argv → queue (frontend drains it on mount).
            if let Some(diff_ref) =
                diff_ref_from_argv(&std::env::args().collect::<Vec<_>>())
            {
                app.state::<PendingDiffs>().push(diff_ref);
            }
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
                            focus_main(&w);
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
        .on_window_event(|window, event| {
            // Closing the last window hides it; the process + tray stay warm.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running gtl-viewer");
}

/// Map a `diff://<repo-id>/<content-hash>` request to the stored HTML bytes.
/// Any malformed/escaping request or read failure yields 404 — never a panic.
fn serve_diff(request: &tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .expect("static 404 builds")
    };
    let uri = request.uri();
    let repo_id = uri.host().unwrap_or_default();
    let hash = uri.path().trim_start_matches('/').trim_end_matches(".html");
    let Some(root) = protocol::store_root() else { return not_found() };
    let Some(path) = protocol::resolve_diff_uri(&root, repo_id, hash) else {
        return not_found();
    };
    match std::fs::read(&path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(bytes)
            .expect("html response builds"),
        Err(_) => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Guards tests that mutate `GIT_TOOLS_DATA_DIR` (process-global env var).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const REPO: &str = "0123456789abcdef";
    const HASH: &str = "fedcba9876543210";

    fn make_request(uri: &str) -> tauri::http::Request<Vec<u8>> {
        tauri::http::Request::builder()
            .uri(uri)
            .body(Vec::new())
            .unwrap()
    }

    #[test]
    fn serve_diff_all_env_cases() {
        let _guard = ENV_LOCK.lock().unwrap();

        // --- Happy path: file exists, expect 200 with correct body + Content-Type ---
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path();
        let dir = store.join("diffs").join(REPO);
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join(format!("{HASH}.html"));
        let content = b"<html>hello</html>";
        std::fs::write(&file_path, content).unwrap();

        // SAFETY: guarded by ENV_LOCK; no other thread touches this var concurrently.
        unsafe { std::env::set_var("GIT_TOOLS_DATA_DIR", store) };

        let req = make_request(&format!("diff://{REPO}/{HASH}"));
        let resp = serve_diff(&req);
        assert_eq!(resp.status(), StatusCode::OK, "happy path: expected 200");
        assert_eq!(resp.body(), content, "happy path: body mismatch");
        assert_eq!(
            resp.headers().get("Content-Type").and_then(|v| v.to_str().ok()),
            Some("text/html; charset=utf-8"),
            "happy path: wrong Content-Type"
        );

        // --- Missing file: valid-shape ids but no file on disk ---
        let req = make_request(&format!("diff://{REPO}/aabbccddeeff0011"));
        let resp = serve_diff(&req);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "missing file: expected 404");

        // --- Multi-segment path attack: trailing segments make hash non-token-shaped ---
        let req = make_request(&format!("diff://{REPO}/{HASH}/extra/segment"));
        let resp = serve_diff(&req);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "multi-segment: expected 404");

        // --- Empty path: diff://<repo>/ ---
        let req = make_request(&format!("diff://{REPO}/"));
        let resp = serve_diff(&req);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "empty path: expected 404");

        // SAFETY: guarded by ENV_LOCK; no other thread touches this var concurrently.
        unsafe { std::env::remove_var("GIT_TOOLS_DATA_DIR") };
    }
}
