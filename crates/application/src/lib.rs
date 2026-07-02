//! Application core (Ports & Adapters "inside"): port traits + vertical
//! slices per feature. Depends only on `domain` and the cqrs trio.

pub mod diffs;
pub mod ports;
pub mod shared;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
