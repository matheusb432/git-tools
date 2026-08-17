//! Process-boundary values that are not application operation requests.
//! Application requests are serialized directly by their presentation layer.

pub mod daemon;
pub mod diffs;
pub mod envelope;
pub mod live_views;
pub mod projects;
pub mod recipes;
pub mod settings;
pub mod tags;
pub mod viewer;

#[cfg(test)]
mod testing;
