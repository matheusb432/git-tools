//! Linux backend: effectful appliers for the PAL port. cfg-selected in `lib.rs`.
pub mod focus;
pub mod spawn;

pub use focus::activate_window;
pub use spawn::{spawn_detached, spawn_detached_in};
