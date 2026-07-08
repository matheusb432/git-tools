//! The recipe DTO: a serializable descriptor of *how to produce* a view —
//! source identity + operation — never view data. Crosses the Tauri IPC
//! boundary and, serialized, the app-history log (`recent_renders.recipe_json`).
//! Serde mirrors of domain types live here because domain carries no serde by
//! design; `Recipe` equality is the tab-dedupe identity.

use std::{num::NonZeroU32, path::PathBuf};

use domain::diffs::DiffTarget;
use serde::{Deserialize, Serialize};

/// The identity of the repository a recipe renders from (the live-view source
/// model: `kind` + `value`; only `LocalRepo` exists today).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RecipeSource {
    LocalRepo(PathBuf),
}

/// Serde mirror of [`DiffTarget`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "kebab-case")]
pub enum RecipeTarget {
    Unpushed,
    Base { rev: String },
    Range { range: String },
    Merge { base: String },
    Last { count: NonZeroU32 },
}

impl From<RecipeTarget> for DiffTarget {
    fn from(target: RecipeTarget) -> Self {
        match target {
            RecipeTarget::Unpushed => Self::Unpushed,
            RecipeTarget::Base { rev } => Self::Base(rev),
            RecipeTarget::Range { range } => Self::Range(range),
            RecipeTarget::Merge { base } => Self::Merge(base),
            RecipeTarget::Last { count } => Self::Last(count),
        }
    }
}

/// Which diff-family operation the recipe runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum RecipeOp {
    Diff { target: RecipeTarget },
    MergeDiff { base: Option<String> },
    SquashPreview,
}

/// One renderable recipe: the repo source plus the operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub source: RecipeSource,
    pub op: RecipeOp,
}

impl Recipe {
    /// The repo directory the compute slices resolve from.
    pub fn cwd(&self) -> PathBuf {
        match &self.source {
            RecipeSource::LocalRepo(path) => path.clone(),
        }
    }

    /// The stable kind tag recorded in app history.
    pub fn kind_tag(&self) -> &'static str {
        match self.op {
            RecipeOp::Diff { .. } => "diff",
            RecipeOp::MergeDiff { .. } => "merge-diff",
            RecipeOp::SquashPreview => "squash-preview",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff_recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed,
            },
        }
    }

    #[test]
    fn recipe_json_shape_is_pinned() {
        // ! This exact string lands in recent_renders.recipe_json — changing it
        // ! breaks reopening of previously recorded history rows.
        let json = serde_json::to_string(&diff_recipe()).unwrap();
        assert_eq!(
            json,
            r#"{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed"}}}"#
        );
    }

    #[test]
    fn recipe_round_trips_through_json() {
        for recipe in [
            diff_recipe(),
            Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
                op: RecipeOp::MergeDiff { base: None },
            },
            Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
                op: RecipeOp::SquashPreview,
            },
        ] {
            let json = serde_json::to_string(&recipe).unwrap();
            let back: Recipe = serde_json::from_str(&json).unwrap();
            assert_eq!(back, recipe);
        }
    }

    #[test]
    fn target_maps_onto_the_domain_target() {
        assert_eq!(
            DiffTarget::from(RecipeTarget::Range {
                range: "a..b".into()
            }),
            DiffTarget::Range("a..b".into())
        );
        assert_eq!(
            DiffTarget::from(RecipeTarget::Unpushed),
            DiffTarget::Unpushed
        );
    }

    #[test]
    fn kind_tag_names_the_operation() {
        assert_eq!(diff_recipe().kind_tag(), "diff");
    }

    #[test]
    fn unknown_source_kind_is_rejected_at_deserialization() {
        let json = r#"{"source":{"kind":"GithubRepo","value":"o/r"},"op":{"op":"squash-preview"}}"#;
        assert!(serde_json::from_str::<Recipe>(json).is_err());
    }
}
