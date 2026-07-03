//! Generic process-startup utilities shared by the binary roots (currently `daemon`).
//!
//! This is a **leaf** crate: it depends only on the async runtime and observability stack
//! (`tokio`, `tracing`, `tracing-subscriber`, `anyhow`) and on **none** of the hexagon's crates
//! (`domain`, `application`, `infra`, `cqrsy`, `contracts`). It carries no business logic and no
//! adapters — only the boot plumbing every process root would otherwise copy verbatim:
//! environment parsing, tracing init, and the graceful-shutdown signal future.
//!
//! Each process root keeps its own config and decides which variables it reads; only the
//! mechanical helpers live here.

mod env;
mod logging;
mod shutdown;

pub use env::{optional_env, parse_env_or};
pub use logging::init_tracing;
pub use shutdown::shutdown_signal;
