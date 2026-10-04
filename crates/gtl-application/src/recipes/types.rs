//! Recipe values describe how to produce a view: source identity plus operation, never view data.
//! The application persists recipes through its history and saved viewer tabs.
//! A computed recipe pins the commits it showed; `Recipe::unpinned()` is the intent that an
//! update resolves again and that tab reuse compares.
//! Every enum serializes its persisted and copied JSON tag in `snake_case`.

use std::num::NonZeroU32;

use gtl_models::paths::{ProjectName, RepositoryRoot};
pub use gtl_models::{
    diffs::{DiffTextId, PinnedRange},
    git::{GitRange, GitRevision},
    recipes::RecipeBatchId,
};
use serde::{Deserialize, Serialize};

/// Where a recipe renders from and how.
///
/// Recipe JSON keeps a `source` tagged `local_repo` or `text` and, for repositories only, an
/// `op`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeSource {
    LocalRepo { root: RepositoryRoot, op: RecipeOp },
    Text(TextRecipeSource),
}

/// Stored diff text and the label its renders show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRecipeSource {
    pub id: DiffTextId,
    pub label: ProjectName,
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
    Commit {
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

/// Which diff-family operation a repository recipe runs.
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

/// One renderable recipe: the source plus its optional tab name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RecipeJson", into = "RecipeJson")]
pub struct Recipe {
    pub source: RecipeSource,
    /// An optional human-readable label for the opened viewer tab.
    pub name: Option<ProjectName>,
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize)]
struct RecipeJson {
    source: RecipeSourceJson,
    op: Option<RecipeOp>,
    name: Option<ProjectName>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum RecipeSourceJson {
    LocalRepo(RepositoryRoot),
    Text(TextRecipeSource),
}

#[derive(Debug, thiserror::Error)]
enum RecipeJsonError {
    #[error("a repository recipe needs an operation")]
    MissingOperation,
    #[error("a text recipe has no operation")]
    UnexpectedOperation,
}

impl TryFrom<RecipeJson> for Recipe {
    type Error = RecipeJsonError;

    fn try_from(json: RecipeJson) -> Result<Self, Self::Error> {
        let source = match (json.source, json.op) {
            (RecipeSourceJson::LocalRepo(root), Some(op)) => RecipeSource::LocalRepo { root, op },
            (RecipeSourceJson::Text(text), None) => RecipeSource::Text(text),
            (RecipeSourceJson::LocalRepo(_), None) => {
                return Err(RecipeJsonError::MissingOperation);
            }
            (RecipeSourceJson::Text(_), Some(_)) => {
                return Err(RecipeJsonError::UnexpectedOperation);
            }
        };
        Ok(Self {
            source,
            name: json.name,
        })
    }
}

impl From<Recipe> for RecipeJson {
    fn from(recipe: Recipe) -> Self {
        let (source, op) = match recipe.source {
            RecipeSource::LocalRepo { root, op } => (RecipeSourceJson::LocalRepo(root), Some(op)),
            RecipeSource::Text(text) => (RecipeSourceJson::Text(text), None),
        };
        Self {
            source,
            op,
            name: recipe.name,
        }
    }
}

impl Recipe {
    /// The repository directory the compute slices resolve from, if the recipe has one.
    #[must_use]
    pub fn cwd(&self) -> Option<&RepositoryRoot> {
        match &self.source {
            RecipeSource::LocalRepo { root, .. } => Some(root),
            RecipeSource::Text(_) => None,
        }
    }

    /// The Git operation a repository recipe runs.
    #[must_use]
    pub fn op(&self) -> Option<&RecipeOp> {
        match &self.source {
            RecipeSource::LocalRepo { op, .. } => Some(op),
            RecipeSource::Text(_) => None,
        }
    }

    /// The repository name, or the label of stored text, shown before a render completes.
    #[must_use]
    pub fn source_name(&self) -> ProjectName {
        match &self.source {
            RecipeSource::LocalRepo { root, .. } => root.project_name(),
            RecipeSource::Text(text) => text.label.clone(),
        }
    }

    /// The stable kind tag recorded in app history.
    #[must_use]
    pub fn kind_tag(&self) -> &'static str {
        match &self.source {
            RecipeSource::LocalRepo {
                op: RecipeOp::Diff { .. },
                ..
            } => "diff",
            RecipeSource::LocalRepo {
                op: RecipeOp::MergeDiff { .. },
                ..
            } => "merge-diff",
            RecipeSource::Text(_) => "text",
        }
    }

    /// The pin-stripped projection used as the viewer's snapshot-tab dedupe
    /// identity: two renders of the same repo + operation match even when their
    /// pinned resolutions differ.
    #[must_use]
    pub fn unpinned(&self) -> Recipe {
        let mut recipe = self.clone();
        match &mut recipe.source {
            RecipeSource::LocalRepo {
                op: RecipeOp::Diff { target },
                ..
            } => match target {
                RecipeTarget::Unpushed { pinned }
                | RecipeTarget::Range { pinned, .. }
                | RecipeTarget::Merge { pinned, .. }
                | RecipeTarget::Last { pinned, .. } => *pinned = None,
                RecipeTarget::Base { .. } | RecipeTarget::Commit { .. } => {}
            },
            RecipeSource::LocalRepo {
                op: RecipeOp::MergeDiff { pinned, .. },
                ..
            } => {
                *pinned = None;
            }
            RecipeSource::Text(_) => {}
        }
        recipe
    }

    /// Whether the recipe's content is fixed rather than resolved again when it renders: exact
    /// commits, or stored text.
    #[must_use]
    pub fn is_pinned(&self) -> bool {
        matches!(self.source, RecipeSource::Text(_)) || *self != self.unpinned()
    }
}

/// Recipes submitted to the viewer together, each opening a snapshot tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeBatch {
    pub batch_id: RecipeBatchId,
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
            source: RecipeSource::LocalRepo {
                root: root("//fixture.invalid/repositories/repos/gt"),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            },
            name: None,
        }
    }

    #[test]
    fn canonical_recipe_json_shape_is_pinned() {
        let json = serde_json::to_string(&diff_recipe()).unwrap();
        assert_eq!(
            json,
            r#"{"source":{"kind":"local_repo","value":"//fixture.invalid/repositories/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed"}}}"#
        );
    }

    #[test]
    fn canonical_merge_operation_tag_is_snake_case() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo {
                root: root("//fixture.invalid/repositories/repos/gt"),
                op: RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
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
                source: RecipeSource::LocalRepo {
                    root: root("//fixture.invalid/repositories/repos/gt"),
                    op: RecipeOp::MergeDiff {
                        base: None,
                        pinned: None,
                    },
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
                source: RecipeSource::LocalRepo {
                    root: root("//fixture.invalid/repositories/repos/gt"),
                    op,
                },
                name: None,
            };

            assert_eq!(recipe.kind_tag(), expected);
        }
    }

    #[test]
    fn text_recipe_json_has_no_operation() {
        let recipe = Recipe {
            source: RecipeSource::Text(TextRecipeSource {
                id: DiffTextId::try_new("a".repeat(64)).unwrap(),
                label: project_name("review.diff"),
            }),
            name: Some(project_name("review")),
        };

        let json = serde_json::to_string(&recipe).unwrap();

        assert_eq!(
            json,
            format!(
                r#"{{"source":{{"kind":"text","value":{{"id":"{}","label":"review.diff"}}}},"name":"review"}}"#,
                "a".repeat(64)
            )
        );
        assert_eq!(serde_json::from_str::<Recipe>(&json).unwrap(), recipe);
        assert!(recipe.is_pinned());
        assert_eq!(recipe.cwd(), None);
    }

    #[test]
    fn operation_must_match_the_source_kind() {
        let text_with_op = format!(
            r#"{{"source":{{"kind":"text","value":{{"id":"{}","label":"x"}}}},"op":{{"op":"merge_diff"}}}}"#,
            "a".repeat(64)
        );
        let repository_without_op =
            r#"{"source":{"kind":"local_repo","value":"//fixture.invalid/repositories/repos/gt"}}"#;

        assert!(serde_json::from_str::<Recipe>(&text_with_op).is_err());
        assert!(serde_json::from_str::<Recipe>(repository_without_op).is_err());
    }

    #[test]
    fn unknown_source_kind_is_rejected_at_deserialization() {
        let json = r#"{"source":{"kind":"github_repo","value":"o/r"},"op":{"op":"merge_diff","base":null}}"#;
        assert!(serde_json::from_str::<Recipe>(json).is_err());
    }

    #[test]
    fn pinned_unpushed_recipe_json_shape_is_pinned() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo {
                root: root("//fixture.invalid/repositories/repos/gt"),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed {
                        pinned: Some(pinned_range(
                            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        )),
                    },
                },
            },
            name: None,
        };
        let json = serde_json::to_string(&recipe).unwrap();
        assert_eq!(
            json,
            r#"{"source":{"kind":"local_repo","value":"//fixture.invalid/repositories/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed","pinned":{"base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}}"#
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
                source: RecipeSource::LocalRepo {
                    root: root("//fixture.invalid/repositories/repos/gt"),
                    op,
                },
                name: Some(project_name("n")),
            };
            let stripped = recipe.unpinned();
            assert!(!serde_json::to_string(&stripped).unwrap().contains("pinned"));
            // Everything except the pin is preserved.
            assert_eq!(stripped.cwd(), recipe.cwd());
            assert_eq!(stripped.name, recipe.name);
        }
    }

    #[test]
    fn unpinned_projection_of_two_different_pins_is_equal() {
        let recipe_with = |head: &str| Recipe {
            source: RecipeSource::LocalRepo {
                root: root("//fixture.invalid/repositories/repos/gt"),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed {
                        pinned: Some(pinned_range(&"a".repeat(40), &head.repeat(40))),
                    },
                },
            },
            name: None,
        };
        assert_ne!(recipe_with("b"), recipe_with("c"));
        assert_eq!(recipe_with("b").unpinned(), recipe_with("c").unpinned());
    }
}
