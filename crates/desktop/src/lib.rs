//! gtl-viewer: the Tauri diff viewer. Hosts the store's self-contained HTML
//! artifacts as browser-style tabs over a scoped `diff://` scheme (ADR-0004).
mod commands;
mod diffs;
mod history;
mod protocol;
mod recipe;
mod tabs;
mod view_dto;

use application::{
    diffs::{
        compute_diff::{ComputeDiff, ComputeDiffHandler},
        compute_merge_diff::{ComputeMergeDiff, ComputeMergeDiffHandler},
        compute_squash_preview::{ComputeSquashPreview, ComputeSquashPreviewHandler},
    },
    history::{
        list::{ListHistory, ListHistoryHandler},
        record_render::{RecordRender, RecordRenderHandler},
    },
    live_views::{
        list::{ListLiveViews, ListLiveViewsHandler},
        remove::{RemoveLiveView, RemoveLiveViewHandler},
        save::{SaveLiveView, SaveLiveViewHandler},
    },
    ports::{AppStateStore, ArtifactStore, Clock, DiffSource, RepoProbe},
    settings::{
        get::{GetSetting, GetSettingHandler},
        set::{SetSetting, SetSettingHandler},
    },
};
use cqrsy::Sender;
use diffs::{PendingDiffs, diff_ref_from_argv};
use infra::{
    app_state::SqliteAppState, artifact_store::StoreArtifacts, clock::SystemClock,
    diff_source::GitDiffSource, repo_probe::GitRepoProbe,
};
use tauri::{
    Emitter, Manager, WindowEvent,
    http::{Response, StatusCode},
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

/// Desktop's in-process dispatch facade — one handler field per operation.
/// `#[derive(cqrsy::Mediator)]` implements `Sender<R>` per `#[handles(R)]`
/// field, exactly like `DaemonMediator` in `crates/daemon`. The compute slices
/// are the app render path (structured views, no HTML/store); the daemon stays
/// the boundary for the CLI-facing render paths.
#[derive(Clone, cqrsy::Mediator)]
pub(crate) struct DesktopMediator<S, A, C, P, AS>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
    #[handles(ListHistory)]
    pub list_history: ListHistoryHandler<A>,
    #[handles(ComputeDiff)]
    pub compute_diff: ComputeDiffHandler<S>,
    #[handles(ComputeMergeDiff)]
    pub compute_merge_diff: ComputeMergeDiffHandler<S>,
    #[handles(ComputeSquashPreview)]
    pub compute_squash_preview: ComputeSquashPreviewHandler<S>,
    #[handles(ListLiveViews)]
    pub list_live_views: ListLiveViewsHandler<AS>,
    #[handles(SaveLiveView)]
    pub save_live_view: SaveLiveViewHandler<P, AS, C>,
    #[handles(RemoveLiveView)]
    pub remove_live_view: RemoveLiveViewHandler<AS>,
    #[handles(GetSetting)]
    pub get_setting: GetSettingHandler<AS>,
    #[handles(SetSetting)]
    pub set_setting: SetSettingHandler<AS>,
    #[handles(RecordRender)]
    pub record_render: RecordRenderHandler<AS, C>,
}

/// The production wiring: real adapters end to end (the daemon's adapters plus
/// the `SQLite` app-state store).
pub(crate) type WiredMediator =
    DesktopMediator<GitDiffSource, StoreArtifacts, SystemClock, GitRepoProbe, SqliteAppState>;

fn wired_mediator() -> WiredMediator {
    DesktopMediator {
        list_history: ListHistoryHandler {
            store: StoreArtifacts,
        },
        compute_diff: ComputeDiffHandler {
            source: GitDiffSource,
        },
        compute_merge_diff: ComputeMergeDiffHandler {
            source: GitDiffSource,
        },
        compute_squash_preview: ComputeSquashPreviewHandler {
            source: GitDiffSource,
        },
        list_live_views: ListLiveViewsHandler {
            store: SqliteAppState,
        },
        save_live_view: SaveLiveViewHandler {
            probe: GitRepoProbe,
            store: SqliteAppState,
            clock: SystemClock,
        },
        remove_live_view: RemoveLiveViewHandler {
            store: SqliteAppState,
        },
        get_setting: GetSettingHandler {
            store: SqliteAppState,
        },
        set_setting: SetSettingHandler {
            store: SqliteAppState,
        },
        record_render: RecordRenderHandler {
            store: SqliteAppState,
            clock: SystemClock,
        },
    }
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
        // ! `XlibWindowHandle::window` is `c_ulong` — u64 on Linux but u32 on Windows (LLP64),
        // ! so this `.into()` is a real cross-platform widening, not the no-op it looks like here.
        #[allow(clippy::useless_conversion)]
        RawWindowHandle::Xlib(h) => Some(h.window.into()),
        RawWindowHandle::Xcb(h) => Some(u64::from(h.window.get())),
        _ => None,
    }
}

/// Frontend pulls queued diff refs on mount (cold-start + any that arrived first).
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri's #[command] extractors must be taken by value"
)]
fn drain_pending_diffs(state: tauri::State<'_, PendingDiffs>) -> Vec<String> {
    state.drain()
}

/// Return all stored diff previews sorted newest-first, for the history panel.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri's #[command] extractors must be taken by value"
)]
fn list_history(state: tauri::State<'_, WiredMediator>) -> Vec<history::HistoryEntry> {
    let Some(store_root) = protocol::store_root() else {
        return Vec::new();
    };
    state
        .send_now(ListHistory { store_root })
        .map(|resp| resp.entries.into_iter().map(history::to_entry).collect())
        .unwrap_or_default()
}

/// Build and run the Tauri application. Called by `main.rs`.
///
/// # Panics
/// Panics when the Tauri runtime fails to build or start (no display, broken
/// webview install) — fatal for a desktop app, so it surfaces as a crash.
pub fn run() {
    tauri::Builder::default()
        .manage(wired_mediator())
        .manage(tabs::RenderedTabs::default())
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
        .invoke_handler(tauri::generate_handler![
            drain_pending_diffs,
            list_history,
            commands::tabs::open_recipe,
            commands::tabs::tab_meta,
            commands::tabs::file_rows,
            commands::tabs::refresh_tab,
            commands::tabs::close_tab,
            commands::app_state::list_live_views,
            commands::app_state::save_live_view,
            commands::app_state::remove_live_view,
            commands::app_state::get_setting,
            commands::app_state::set_setting,
        ])
        .register_asynchronous_uri_scheme_protocol("diff", |_ctx, request, responder| {
            responder.respond(serve_diff(&request));
        })
        .setup(|app| {
            // Cold-start argv → queue (frontend drains it on mount).
            if let Some(diff_ref) = diff_ref_from_argv(&std::env::args().collect::<Vec<_>>()) {
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
    let Some(root) = protocol::store_root() else {
        return not_found();
    };
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
pub(crate) mod test_support {
    use application::testing::{
        FakeDiffSource, FakeRepoProbe, FixedClock, InMemoryAppStateStore, InMemoryArtifactStore,
    };

    use super::*;

    pub(crate) type FakeMediator = DesktopMediator<
        FakeDiffSource,
        InMemoryArtifactStore,
        FixedClock,
        FakeRepoProbe,
        InMemoryAppStateStore,
    >;

    pub(crate) fn fake_mediator(source: FakeDiffSource) -> FakeMediator {
        fake_mediator_parts(
            source,
            InMemoryArtifactStore::default(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        )
    }

    pub(crate) fn fake_mediator_with(
        source: FakeDiffSource,
        store: InMemoryArtifactStore,
        app_state: InMemoryAppStateStore,
    ) -> FakeMediator {
        fake_mediator_parts(source, store, app_state, FakeRepoProbe::default())
    }

    pub(crate) fn fake_mediator_with_probe(
        source: FakeDiffSource,
        probe: FakeRepoProbe,
    ) -> FakeMediator {
        fake_mediator_parts(
            source,
            InMemoryArtifactStore::default(),
            InMemoryAppStateStore::default(),
            probe,
        )
    }

    fn fake_mediator_parts(
        source: FakeDiffSource,
        store: InMemoryArtifactStore,
        app_state: InMemoryAppStateStore,
        probe: FakeRepoProbe,
    ) -> FakeMediator {
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        DesktopMediator {
            list_history: ListHistoryHandler { store },
            compute_diff: ComputeDiffHandler {
                source: source.clone(),
            },
            compute_merge_diff: ComputeMergeDiffHandler {
                source: source.clone(),
            },
            compute_squash_preview: ComputeSquashPreviewHandler { source },
            list_live_views: ListLiveViewsHandler {
                store: app_state.clone(),
            },
            save_live_view: SaveLiveViewHandler {
                probe,
                store: app_state.clone(),
                clock: clock.clone(),
            },
            remove_live_view: RemoveLiveViewHandler {
                store: app_state.clone(),
            },
            get_setting: GetSettingHandler {
                store: app_state.clone(),
            },
            set_setting: SetSettingHandler {
                store: app_state.clone(),
            },
            record_render: RecordRenderHandler {
                store: app_state,
                clock,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

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
            resp.headers()
                .get("Content-Type")
                .and_then(|v| v.to_str().ok()),
            Some("text/html; charset=utf-8"),
            "happy path: wrong Content-Type"
        );

        // --- Missing file: valid-shape ids but no file on disk ---
        let req = make_request(&format!("diff://{REPO}/aabbccddeeff0011"));
        let resp = serve_diff(&req);
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "missing file: expected 404"
        );

        // --- Multi-segment path attack: trailing segments make hash non-token-shaped ---
        let req = make_request(&format!("diff://{REPO}/{HASH}/extra/segment"));
        let resp = serve_diff(&req);
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "multi-segment: expected 404"
        );

        // --- Empty path: diff://<repo>/ ---
        let req = make_request(&format!("diff://{REPO}/"));
        let resp = serve_diff(&req);
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "empty path: expected 404"
        );

        // SAFETY: guarded by ENV_LOCK; no other thread touches this var concurrently.
        unsafe { std::env::remove_var("GIT_TOOLS_DATA_DIR") };
    }

    #[test]
    fn desktop_mediator_dispatches_list_history_end_to_end() {
        use application::{
            ports::HistoryRecord,
            testing::{FakeDiffSource, InMemoryArtifactStore},
        };
        use domain::diffs::DiffKind;

        let record = HistoryRecord {
            repo_id: "repo1".into(),
            repo_name: "n".into(),
            title: "t".into(),
            range_label: "x".into(),
            head_committed_at: "2026-01-01T00:00:00Z".into(),
            generated_at: "2026-01-01T00:00:00Z".into(),
            content_hash: "h".into(),
            kind: DiffKind::TwoDot,
            byte_size: 0,
        };
        let mediator = test_support::fake_mediator_with(
            FakeDiffSource::default(),
            InMemoryArtifactStore {
                history: vec![record.clone()],
                ..Default::default()
            },
            application::testing::InMemoryAppStateStore::default(),
        );

        let response = mediator
            .send_now(ListHistory {
                store_root: "/store".into(),
            })
            .expect("dispatch succeeds");

        assert_eq!(response.entries.len(), 1);
        assert_eq!(response.entries[0].repo_id, "repo1");
    }

    #[test]
    fn desktop_mediator_dispatches_compute_diff_end_to_end() {
        use application::testing::FakeDiffSource;

        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![domain::diffs::Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                ..Default::default()
            }],
            diff_output: "diff --git a/f.txt b/f.txt\n@@ -1 +1 @@\n-a\n+b\n".into(),
            ..Default::default()
        };
        let mediator = test_support::fake_mediator(source);

        let response = mediator
            .send_now(ComputeDiff {
                cwd: "/repo".into(),
                target: domain::diffs::DiffTarget::Unpushed,
            })
            .expect("dispatch succeeds");

        assert_eq!(response.view.files.len(), 1);
    }
}
