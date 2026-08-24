//! Snapshot recipe construction and immutable Git-range pinning operations.

pub mod build_recipe;
pub mod pin_recipe;
mod resolution;
mod types;

pub(crate) use resolution::build_resolved;
pub use types::{
    PinnedRange, Recipe, RecipeBatch, RecipeBatchId, RecipeBatchKind, RecipeOp, RecipeSource,
    RecipeTarget,
};
