//! Linux backend: effectful appliers for the PAL port. cfg-selected in `lib.rs`.
pub mod spawn;

pub use spawn::spawn_detached;
