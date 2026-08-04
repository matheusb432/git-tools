//! Snapshot recipe construction and immutable Git-range pinning operations.

pub mod build_managed;
pub mod build_subrepos;
mod dto;
mod logic;
pub mod pin;

pub use dto::RecipeRequest;
