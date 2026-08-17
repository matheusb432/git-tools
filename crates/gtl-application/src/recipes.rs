//! Snapshot recipe construction and immutable Git-range pinning operations.

pub mod pin_recipe;
mod resolution;

pub(crate) use resolution::build_resolved;
