//! Recipe values describe how to produce a view: source identity plus operation, never view data.
//! The application persists recipes through its history and live-view operations.
//! `Recipe::unpinned()` equality is the snapshot-tab dedupe identity (pins
//! differ across runs of the same repo + operation); live recipes are always
//! unpinned, so full `Recipe` equality still governs their identity.
//! Every enum serializes its persisted and copied JSON tag in `snake_case`.

use std::num::NonZeroU32;

use gtl_models::paths::{ProjectName, RepositoryRoot};
pub use gtl_models::{
    diffs::PinnedRange,
    git::{GitRange, GitRevision},
    recipes::RecipeBatchId,
};
use serde::{Deserialize, Serialize};

/// The identity of the repository a recipe renders from.
///
/// Recipe JSON uses the `local_repo` tag. The live-view store's separate
/// `source_kind` identity remains the stable `LocalRepo` string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RecipeSource {
    LocalRepo(RepositoryRoot),
}

/// A hand-maintained serde mirror of `gtl_application::diffs::DiffTarget`; the mapping onto the
/// application type lives in the consuming crate.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum RecipeTarget {
    Unpushed {
        pinned: Option<PinnedRange>,
    },
    Base {
        rev: GitRevision,
    },
    Range {
        range: GitRange,
        pinned: Option<PinnedRange>,
    },
    Merge {
        base: GitRevision,
        pinned: Option<PinnedRange>,
    },
    Last {
        count: NonZeroU32,
        pinned: Option<PinnedRange>,
    },
}

/// Which diff-family operation the recipe runs.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RecipeOp {
    Diff {
        target: RecipeTarget,
    },
    MergeDiff {
        base: Option<GitRevision>,
        pinned: Option<PinnedRange>,
    },
}

/// One renderable recipe: the repo source plus the operation.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub source: RecipeSource,
    pub op: RecipeOp,
    /// An optional human-readable label for the opened viewer tab.
    pub name: Option<ProjectName>,
}

impl Recipe {
    /// The repo directory the compute slices resolve from.
    #[must_use]
    pub fn cwd(&self) -> RepositoryRoot {
        match &self.source {
            RecipeSource::LocalRepo(path) => path.clone(),
        }
    }

    /// The stable kind tag recorded in app history.
    #[must_use]
    pub fn kind_tag(&self) -> &'static str {
        match self.op {
            RecipeOp::Diff { .. } => "diff",
            RecipeOp::MergeDiff { .. } => "merge-diff",
        }
    }

    /// The pin-stripped projection used as the viewer's snapshot-tab dedupe
    /// identity: two renders of the same repo + operation match even when their
    /// pinned resolutions differ.
    #[must_use]
    pub fn unpinned(&self) -> Recipe {
        let mut recipe = self.clone();
        match &mut recipe.op {
            RecipeOp::Diff { target } => match target {
                RecipeTarget::Unpushed { pinned }
                | RecipeTarget::Range { pinned, .. }
                | RecipeTarget::Merge { pinned, .. }
                | RecipeTarget::Last { pinned, .. } => *pinned = None,
                RecipeTarget::Base { .. } => {}
            },
            RecipeOp::MergeDiff { pinned, .. } => {
                *pinned = None;
            }
        }
        recipe
    }
}

/// Identifies how every recipe in a [`RecipeBatch`] behaves in the viewer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RecipeBatchKind {
    /// Opens immutable snapshot tabs.
    #[default]
    Snapshot,
    /// Opens persisted, refreshable live tabs.
    Live,
}

/// A homogeneous batch of recipes submitted to the viewer together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeBatch {
    pub batch_id: RecipeBatchId,
    /// The viewer behavior shared by every recipe in this batch.
    pub kind: RecipeBatchKind,
    pub recipes: Vec<Recipe>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::pinned_range;

    fn root(path: &str) -> RepositoryRoot {
        RepositoryRoot::try_new(path.into()).unwrap()
    }

    fn project_name(name: &str) -> ProjectName {
        ProjectName::try_new(name.to_owned()).unwrap()
    }

    fn revision(raw: &str) -> GitRevision {
        GitRevision::try_new(raw.to_owned()).unwrap()
    }

    fn range(raw: &str) -> GitRange {
        GitRange::try_new(raw.to_owned()).unwrap()
    }

    fn diff_recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(root("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    #[test]
    fn canonical_recipe_json_shape_is_pinned() {
        let json = serde_json::to_string(&diff_recipe()).unwrap();
        assert_eq!(
            json,
            r#"{"source":{"kind":"local_repo","value":"/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed"}}}"#
        );
    }

    #[test]
    fn canonical_merge_operation_tag_is_snake_case() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo(root("/repos/gt")),
            op: RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            name: None,
        };

        let json = serde_json::to_value(recipe).unwrap();

        assert_eq!(json["op"]["op"], "merge_diff");
    }

    #[test]
    fn recipe_round_trips_through_json() {
        for recipe in [
            diff_recipe(),
            Recipe {
                source: RecipeSource::LocalRepo(root("/repos/gt")),
                op: RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                name: None,
            },
        ] {
            let json = serde_json::to_string(&recipe).unwrap();
            let back: Recipe = serde_json::from_str(&json).unwrap();
            assert_eq!(back, recipe);
        }
    }

    #[test]
    fn kind_tag_names_the_operation() {
        for (op, expected) in [
            (
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                "diff",
            ),
            (
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                "merge-diff",
            ),
        ] {
            let recipe = Recipe {
                source: RecipeSource::LocalRepo(root("/repos/gt")),
                op,
                name: None,
            };

            assert_eq!(recipe.kind_tag(), expected);
        }
    }

    #[test]
    fn unknown_source_kind_is_rejected_at_deserialization() {
        let json = r#"{"source":{"kind":"github_repo","value":"o/r"},"op":{"op":"merge_diff","base":null}}"#;
        assert!(serde_json::from_str::<Recipe>(json).is_err());
    }

    #[test]
    fn pinned_unpushed_recipe_json_shape_is_pinned() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo(root("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(pinned_range(
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    )),
                },
            },
            name: None,
        };
        let json = serde_json::to_string(&recipe).unwrap();
        assert_eq!(
            json,
            r#"{"source":{"kind":"local_repo","value":"/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed","pinned":{"base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}}"#
        );
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(back, recipe);
    }

    #[test]
    fn unpinned_projection_strips_every_pin() {
        let pin = Some(pinned_range(&"a".repeat(40), &"b".repeat(40)));
        let cases = vec![
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: pin.clone(),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: range("x..y"),
                    pinned: pin.clone(),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: revision("main"),
                    pinned: pin.clone(),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Last {
                    count: NonZeroU32::new(2).unwrap(),
                    pinned: pin.clone(),
                },
            },
            RecipeOp::MergeDiff {
                base: Some(revision("main")),
                pinned: pin.clone(),
            },
        ];
        for op in cases {
            let recipe = Recipe {
                source: RecipeSource::LocalRepo(root("/repos/gt")),
                op,
                name: Some(project_name("n")),
            };
            let stripped = recipe.unpinned();
            assert!(!serde_json::to_string(&stripped).unwrap().contains("pinned"));
            // Everything except the pin is preserved.
            assert_eq!(stripped.source, recipe.source);
            assert_eq!(stripped.name, recipe.name);
        }
    }

    #[test]
    fn unpinned_projection_of_two_different_pins_is_equal() {
        let recipe_with = |head: &str| Recipe {
            source: RecipeSource::LocalRepo(root("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(pinned_range(&"a".repeat(40), &head.repeat(40))),
                },
            },
            name: None,
        };
        assert_ne!(recipe_with("b"), recipe_with("c"));
        assert_eq!(recipe_with("b").unpinned(), recipe_with("c").unpinned());
    }
}
