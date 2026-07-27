//! Process-boundary values that are not application operation requests.
//! Application requests are serialized directly by their presentation layer.

pub mod diffs;
pub mod envelope;
pub mod live_views;
pub mod managed;
pub mod recipes;
