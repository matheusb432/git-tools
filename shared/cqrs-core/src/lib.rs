//! Zero-cost CQRS contracts — [`Request`]/[`RequestHandler`] dispatch and the
//! [`Behavior`]/[`Pipeline`] middleware model.
//!
//! # Design
//!
//! Edition 2024 stable `async fn` in traits gives zero-cost monomorphization: each concrete
//! handler and behavior gets its own future, no boxing, no `dyn`. Cross-cutting concerns are
//! [`Behavior`]s in MediatR's shape (`handle(req, next)`), composed statically as nested
//! zero-sized tuples ([`Pipeline`]) and run by [`dispatch`] on mediator dispatch only.
//!
//! This crate is contracts-only and dependency-free. Behavior *implementations* are
//! application policy and belong in the consumer's application layer.

mod behavior;
mod handler;
mod pipeline;

pub use behavior::{Behavior, Next};
pub use handler::{Request, RequestHandler};
pub use pipeline::{Pipeline, dispatch};
