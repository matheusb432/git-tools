use gtl_wire::window::{WindowAction, WindowState};

use crate::ViewerClientError;

#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub async fn state() -> Result<WindowState, ViewerClientError> {
    crate::viewer::tauri::invoke_without_arguments("desktop_window_state").await
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub async fn perform(action: WindowAction) -> Result<(), ViewerClientError> {
    crate::viewer::tauri::invoke_with_request("desktop_window_action", action).await
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-ipc")))]
pub fn state() -> impl std::future::Future<Output = Result<WindowState, ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Unavailable))
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-ipc")))]
pub fn perform(
    _action: WindowAction,
) -> impl std::future::Future<Output = Result<(), ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Unavailable))
}
