//! Snapshot recipe construction and immutable Git-range pinning operations.

pub mod build_managed;
pub mod build_subrepos;
pub mod pin;
mod resolution;

pub(crate) use resolution::build_resolved;
