#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(super) fn remap_unfocused_wayland_window(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle};

    let wayland = window
        .display_handle()
        .is_ok_and(|handle| matches!(handle.as_raw(), RawDisplayHandle::Wayland(_)));
    if wayland && window.is_visible()? && !window.is_focused()? {
        // GNOME can reject repeat activation tokens; remapping retains the existing WebView.
        window.hide()?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) fn activate(window_id: u64) {
    linux::activate(window_id);
}

#[cfg(not(target_os = "linux"))]
pub(super) fn activate(_window_id: u64) {}
