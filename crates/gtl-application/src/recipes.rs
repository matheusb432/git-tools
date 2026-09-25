//! Snapshot recipe construction and immutable Git-range pinning operations.

pub mod build_recipe;
pub mod pin_recipe;
pub(crate) mod recipe_label;
mod resolution;
mod types;

pub use recipe_label::RecipeLabelParts;
pub(crate) use resolution::build_resolved;
pub use types::{
    PinnedRange, Recipe, RecipeBatch, RecipeBatchId, RecipeBatchKind, RecipeOp, RecipeSource,
    RecipeTarget,
};
