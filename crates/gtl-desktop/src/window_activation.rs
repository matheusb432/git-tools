#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(super) fn activate(window_id: u64) {
    linux::activate(window_id);
}

#[cfg(not(target_os = "linux"))]
pub(super) fn activate(_window_id: u64) {}
