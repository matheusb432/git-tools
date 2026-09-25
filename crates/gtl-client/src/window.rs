use gtl_wire::{
    viewer::ViewerTheme,
    window::{TrayLabels, WindowAction, WindowState},
};

use crate::ViewerClientError;

#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub async fn set_scale(
    scale: gtl_wire::window::ViewerScalePercent,
) -> Result<(), ViewerClientError> {
    crate::viewer::tauri::invoke_with_request("desktop_window_scale", scale).await
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-ipc")))]
pub fn set_scale(
    _scale: gtl_wire::window::ViewerScalePercent,
) -> impl std::future::Future<Output = Result<(), ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Disconnected))
}

#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub async fn set_tray_labels(labels: TrayLabels) -> Result<(), ViewerClientError> {
    crate::viewer::tauri::invoke_with_request("desktop_tray_labels", labels).await
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-ipc")))]
pub fn set_tray_labels(
    _labels: TrayLabels,
) -> impl std::future::Future<Output = Result<(), ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Disconnected))
}

/// Draws the tray and installed launcher icons in the viewer theme.
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub async fn set_theme_icons(theme: ViewerTheme) -> Result<(), ViewerClientError> {
    crate::viewer::tauri::invoke_with_request("desktop_theme_icons", theme).await
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-ipc")))]
pub fn set_theme_icons(
    _theme: ViewerTheme,
) -> impl std::future::Future<Output = Result<(), ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Disconnected))
}

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
    std::future::ready(Err(ViewerClientError::Disconnected))
}

#[cfg(not(all(target_arch = "wasm32", feature = "viewer-ipc")))]
pub fn perform(
    _action: WindowAction,
) -> impl std::future::Future<Output = Result<(), ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Disconnected))
}
