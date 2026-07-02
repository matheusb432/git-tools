//! Tracing-subscriber initialisation shared by the process roots.

/// Installs the global `tracing` subscriber once. Honours `RUST_LOG`; defaults to `info`.
///
/// Uses `try_init`, so a second call (e.g. from a test process that already initialised tracing)
/// is a no-op rather than a panic.
pub fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init();
}
