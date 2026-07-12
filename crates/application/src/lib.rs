//! Application core (Ports & Adapters "inside"): ports plus vertical slices.
//!
//! Each feature operation owns its request and one public fused
//! `#[cqrsy::handler(command|query)]` function. Cqrsy generates the request and
//! handler contracts; process roots and tests dispatch through [`cqrsy::Sender`].
//! This crate depends only on `domain` and `cqrsy`.

pub mod diffs;
pub mod history;
pub mod live_views;
pub mod managed;
pub mod ports;
pub mod settings;
pub mod shared;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
