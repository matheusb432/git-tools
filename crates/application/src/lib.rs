//! Application core (Ports & Adapters "inside"): port traits + vertical
//! slices per feature. Depends only on `domain` and `cqrsy`.

pub mod diffs;
pub mod history;
pub mod live_views;
pub mod managed;
pub mod ports;
pub mod settings;
pub mod shared;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
