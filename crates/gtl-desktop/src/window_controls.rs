use gtl_wire::window::{WindowAction, WindowState};

pub(super) const CUSTOM_TITLEBAR: bool = cfg!(any(target_os = "linux", target_os = "windows"));

#[tauri::command]
pub(super) async fn desktop_window_state(
    window: tauri::WebviewWindow,
) -> Result<WindowState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        window.is_maximized().map(|maximized| WindowState {
            custom_titlebar: CUSTOM_TITLEBAR,
            maximized,
        })
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(super) async fn desktop_window_action(
    window: tauri::WebviewWindow,
    request: WindowAction,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || match request {
        WindowAction::Minimize => window.minimize(),
        WindowAction::ToggleMaximize => window.is_maximized().and_then(|maximized| {
            if maximized {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }),
        WindowAction::Close => window.close(),
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}
