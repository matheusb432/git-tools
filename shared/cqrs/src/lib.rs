//! Facade for the zero-cost CQRS engine: re-exports the [`cqrs_core`] contracts, plus the
//! `Request`/`Mediator` derives behind the (non-default) `derive` feature.
//!
//! Depend on this crate only: `cqrs = { path = "…", features = ["derive"] }`. Generated code
//! emits `::cqrs::` paths, so this facade must be in scope under the name `cqrs`.
//! See README.md for the crate's intent and the core/macros/facade split.

pub use cqrs_core::*;
#[cfg(feature = "derive")]
pub use cqrs_macros::{Mediator, Request};
