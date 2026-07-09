//! gtl-viewer: the Tauri diff viewer. Renders diff recipes natively as
//! computed tabs — see `tabs`/`commands::tabs` — with no raw-HTML artifact
//! viewing surface (that surface was retired; `--raw` still renders to the
//! store and opens in the browser, but the desktop GUI never shows it).
mod commands;
mod diffs;
mod recipe;
mod tabs;
mod view_dto;

use application::{
    diffs::{
        compute_diff::{ComputeDiff, ComputeDiffHandler},
        compute_merge_diff::{ComputeMergeDiff, ComputeMergeDiffHandler},
        compute_squash_preview::{ComputeSquashPreview, ComputeSquashPreviewHandler},
    },
    history::record_render::{RecordRender, RecordRenderHandler},
    live_views::{
        list::{ListLiveViews, ListLiveViewsHandler},
        probe::{ProbeSource, ProbeSourceHandler},
        remove::{RemoveLiveView, RemoveLiveViewHandler},
        save::{SaveLiveView, SaveLiveViewHandler},
    },
    ports::{AppStateStore, Clock, DiffSource, RepoProbe},
    settings::{
        get::{GetSetting, GetSettingHandler},
        set::{SetSetting, SetSettingHandler},
    },
};
use diffs::{PendingRecipes, recipes_from_argv};
use gtl_recipe::OpenRecipes;
use infra::{
    app_state::SqliteAppState, clock::SystemClock, diff_source::GitDiffSource,
    repo_probe::GitRepoProbe,
};
use tauri::{
    Emitter, Manager, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

/// Desktop's in-process dispatch facade — one handler field per operation.
/// `#[derive(cqrsy::Mediator)]` implements `Sender<R>` per `#[handles(R)]`
/// field, exactly like `DaemonMediator` in `crates/daemon`. The compute slices
/// are the app render path (structured views, no HTML/store); the daemon stays
/// the boundary for the CLI-facing render paths.
#[derive(Clone, cqrsy::Mediator)]
pub(crate) struct DesktopMediator<S, C, P, AS>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
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
    #[handles(ProbeSource)]
    pub probe_source: ProbeSourceHandler<P>,
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
    DesktopMediator<GitDiffSource, SystemClock, GitRepoProbe, SqliteAppState>;

fn wired_mediator() -> WiredMediator {
    DesktopMediator {
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
        probe_source: ProbeSourceHandler {
            probe: GitRepoProbe,
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

/// Frontend pulls queued recipe batches on mount (cold-start + any that
/// arrived first).
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri's #[command] extractors must be taken by value"
)]
fn drain_pending_recipes(state: tauri::State<'_, PendingRecipes>) -> Vec<OpenRecipes> {
    state.drain()
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
        .manage(PendingRecipes::default())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Second launch: the frontend is already mounted (drains its queue exactly once,
            // at mount), so just emit the batch directly rather than also queueing it in
            // `PendingRecipes` — a queued copy here would never be drained and leak for the
            // life of the process. Cold start (below, in `setup`) still queues: that frontend
            // isn't mounted yet.
            if let Some(batch) = recipes_from_argv(&argv) {
                let _ = app.emit("open-recipe", batch);
            }
            if let Some(win) = app.get_webview_window("main") {
                focus_main(&win);
            }
        }))
        .invoke_handler(tauri::generate_handler![
            drain_pending_recipes,
            commands::tabs::open_recipe,
            commands::tabs::tab_meta,
            commands::tabs::file_rows,
            commands::tabs::refresh_tab,
            commands::tabs::close_tab,
            commands::app_state::list_live_views,
            commands::app_state::save_live_view,
            commands::app_state::probe_source,
            commands::app_state::remove_live_view,
            commands::app_state::get_setting,
            commands::app_state::set_setting,
        ])
        .setup(|app| {
            // Cold-start argv → queue (frontend drains it on mount).
            let argv = std::env::args().collect::<Vec<_>>();
            if let Some(batch) = recipes_from_argv(&argv) {
                app.state::<PendingRecipes>().push(batch);
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

#[cfg(test)]
pub(crate) mod test_support {
    use application::testing::{FakeDiffSource, FakeRepoProbe, FixedClock, InMemoryAppStateStore};

    use super::*;

    pub(crate) type FakeMediator =
        DesktopMediator<FakeDiffSource, FixedClock, FakeRepoProbe, InMemoryAppStateStore>;

    pub(crate) fn fake_mediator(source: FakeDiffSource) -> FakeMediator {
        fake_mediator_parts(
            source,
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        )
    }

    pub(crate) fn fake_mediator_with(
        source: FakeDiffSource,
        app_state: InMemoryAppStateStore,
    ) -> FakeMediator {
        fake_mediator_parts(source, app_state, FakeRepoProbe::default())
    }

    pub(crate) fn fake_mediator_with_probe(
        source: FakeDiffSource,
        probe: FakeRepoProbe,
    ) -> FakeMediator {
        fake_mediator_parts(source, InMemoryAppStateStore::default(), probe)
    }

    fn fake_mediator_parts(
        source: FakeDiffSource,
        app_state: InMemoryAppStateStore,
        probe: FakeRepoProbe,
    ) -> FakeMediator {
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        DesktopMediator {
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
                probe: probe.clone(),
                store: app_state.clone(),
                clock: clock.clone(),
            },
            probe_source: ProbeSourceHandler { probe },
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
    use cqrsy::Sender;

    use super::*;

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
