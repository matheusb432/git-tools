//! Graceful-shutdown signal future shared by the process roots.

/// Returns a future that resolves when the process receives `SIGINT` (Ctrl+C).
///
/// Cross-platform via [`tokio::signal::ctrl_c`], with **no** OS `#[cfg]` — this crate lives under
/// `shared/` and is scanned by the PAL cfg-guard (ADR-0003), which forbids OS `#[cfg]` outside
/// `gtl-platform`. SIGTERM is intentionally not handled here to keep the crate OS-cfg-free; the
/// daemon's primary shutdown paths are the `POST /shutdown` endpoint and the idle-exit timer, with
/// Ctrl+C covering interactive foreground runs.
///
/// The `io::Result<()>` output lets callers that need it observe a handler error;
/// `axum::serve(..).with_graceful_shutdown` callers can discard the result.
pub fn shutdown_signal() -> impl std::future::Future<Output = std::io::Result<()>> {
    async {
        let result = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received (SIGINT)");
        result
    }
}
