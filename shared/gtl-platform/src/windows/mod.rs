//! Windows backend: effectful appliers for the PAL port, cfg-selected in `lib.rs`.
//! Only `spawn` is genuinely OS-bound here: `%LOCALAPPDATA%` paths resolve via the
//! `directories` crate (`paths.rs`) and the `explorer.exe` opener via `policy.rs`,
//! both pure and OS-total, so neither needs a Windows applier.
pub mod spawn;

pub use spawn::{spawn_detached, spawn_detached_in};

/// Window activation is a no-op on Windows: Tauri's `set_focus` owns foreground
/// raising, and the desktop crate's `window_xid` only yields X11 handles, so the
/// EWMH path the Linux backend uses is never reached here.
pub fn activate_window(_xid: u64) {}
