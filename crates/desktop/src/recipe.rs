//! The recipe DTO: a serializable descriptor of *how to produce* a view —
//! source identity + operation — never view data. Crosses the Tauri IPC
//! boundary and, serialized, the app-history log (`recent_renders.recipe_json`).
//! Now defined in the shared `gtl-recipe` crate so the CLI can depend on it too
//! (Phase 5: argv-token handoff to the single-instance viewer) without pulling
//! in Tauri.

// * `RecipeSource` is only consumed by this crate's `#[cfg(test)]` modules, and
// * `OpenRecipes` is re-exported ahead of the Phase 5 viewer-side wiring that
// * will consume it. A plain, non-test `cargo build` of this crate sees neither.
#[allow(unused_imports)]
pub use gtl_recipe::{OpenRecipes, Recipe, RecipeOp, RecipeSource, RecipeTarget};
