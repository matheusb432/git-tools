//! Linux backend: effectful appliers for the PAL port. cfg-selected in `lib.rs`.
pub mod current_executable;
pub mod focus;
pub mod spawn;

pub use current_executable::copy_current_executable;
pub use focus::activate_window;
pub use spawn::{spawn_detached, spawn_detached_in};
