//! Application core (Ports & Adapters "inside"): ports plus vertical slices.
//!
//! Each feature operation owns its request and one public request-first
//! `#[cqrsy::handler(command|query)]` `execute` function. Callers import the
//! operation module and invoke `operation::execute(...)` directly.
//! This crate depends only on `domain` and `cqrsy`.

pub mod diffs;
pub mod history;
pub mod live_views;
pub mod managed;
pub mod ports;
pub mod settings;
pub mod shared;
pub mod viewer;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
