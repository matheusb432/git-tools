//! Recipe wire values describe *how to produce* a view -
//! source identity + operation — never view data. Crosses the Tauri IPC
//! boundary as JSON; the app-history log persists it relationally through
//! `gtl_application::history`.
//! `Recipe::unpinned()` equality is the snapshot-tab dedupe identity (pins
//! differ across runs of the same repo + operation); live recipes are always
//! unpinned, so full `Recipe` equality still governs their identity.
//!
//! This module is app-agnostic: it holds pure serde DTOs and their codec. [`RecipeTarget`] mirrors
//! `gtl_application::diffs::DiffTarget`'s shape. Process roots map application targets into the
//! wire type, and the application viewer maps them back before computing a diff.
//!
//! This module also hosts the argv-token codec Phase 5 uses to hand a batch of
//! recipes from the CLI to the single-instance viewer: [`OpenRecipes`] is a
//! named batch, [`encode_token`]/[`decode_token`] round-trip it through a
//! `gtl-recipe://`-prefixed, base64url-encoded argv string.
//!
//! Every enum serializes its wire tag in `snake_case`. Deserialization also
//! accepts the legacy `PascalCase` source and `kebab-case` operation tags so
//! argv tokens remain readable across the migration.

use std::num::NonZeroU32;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
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
    #[serde(alias = "LocalRepo")]
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
    #[serde(alias = "merge-diff")]
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
    pub fn cwd(&self) -> RepositoryRoot {
        match &self.source {
            RecipeSource::LocalRepo(path) => path.clone(),
        }
    }

    /// The stable kind tag recorded in app history.
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

/// Identifies how every recipe in an [`OpenRecipes`] batch behaves in the viewer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeBatchKind {
    /// Opens immutable snapshot tabs.
    #[default]
    Snapshot,
    /// Opens persisted, refreshable live tabs.
    Live,
}

fn is_default<Value>(value: &Value) -> bool
where
    Value: Default + PartialEq,
{
    value == &Value::default()
}

/// A homogeneous batch of recipes to open together — the unit the CLI hands to
/// the single-instance viewer as one argv token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRecipes {
    pub batch_id: RecipeBatchId,
    /// The viewer behavior shared by every recipe in this batch.
    #[serde(default, skip_serializing_if = "is_default")]
    pub kind: RecipeBatchKind,
    pub recipes: Vec<Recipe>,
}

/// The argv-token prefix that marks a `gtl-viewer` command-line argument as an
/// encoded [`OpenRecipes`] batch rather than a plain file path.
pub const RECIPE_TOKEN_PREFIX: &str = "gtl-recipe://";

/// Encodes a recipe batch into one JSON and base64url argv token.
///
/// # Errors
///
/// Returns the JSON serialization error when the batch cannot be encoded.
pub fn encode_token(batch: &OpenRecipes) -> Result<String, serde_json::Error> {
    let json = serde_json::to_vec(batch)?;
    Ok(format!(
        "{RECIPE_TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(json)
    ))
}

/// Decode an argv token produced by [`encode_token`] back into an
/// [`OpenRecipes`] batch. Returns `None` on any mismatch — missing prefix,
/// invalid base64url, or invalid JSON — and never panics, since the token
/// arrives as untrusted argv input.
pub fn decode_token(token: &str) -> Option<OpenRecipes> {
    let encoded = token.strip_prefix(RECIPE_TOKEN_PREFIX)?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::pinned_range;

    fn root(path: &str) -> RepositoryRoot {
        RepositoryRoot::try_new(path.into()).expect("fixture repository root is absolute")
    }

    fn project_name(name: &str) -> ProjectName {
        ProjectName::try_new(name.to_owned()).expect("fixture project name is non-empty")
    }

    const BATCH_ID: &str = "0198a859-7c4e-7e5f-9e63-ec7bb768d841";

    fn batch_id() -> RecipeBatchId {
        BATCH_ID.parse().expect("fixture batch ID is valid")
    }

    fn revision(raw: &str) -> GitRevision {
        GitRevision::try_new(raw.to_owned()).expect("fixture Git revision is non-empty")
    }

    fn range(raw: &str) -> GitRange {
        GitRange::try_new(raw.to_owned()).expect("fixture Git range is non-empty")
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

    fn sample_batch() -> OpenRecipes {
        OpenRecipes {
            batch_id: batch_id(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![diff_recipe()],
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

        let json = serde_json::to_value(recipe).expect("recipe serializes");

        assert_eq!(json["op"]["op"], "merge_diff");
    }

    #[test]
    fn every_legacy_recipe_enum_tag_still_deserializes() {
        let source: RecipeSource =
            serde_json::from_str(r#"{"kind":"LocalRepo","value":"/repos/gt"}"#)
                .expect("legacy source tag remains readable");
        assert_eq!(source, RecipeSource::LocalRepo(root("/repos/gt")));

        for (json, expected) in [
            (
                r#"{"target":"unpushed"}"#,
                RecipeTarget::Unpushed { pinned: None },
            ),
            (
                r#"{"target":"base","rev":"HEAD"}"#,
                RecipeTarget::Base {
                    rev: revision("HEAD"),
                },
            ),
            (
                r#"{"target":"range","range":"a..b"}"#,
                RecipeTarget::Range {
                    range: range("a..b"),
                    pinned: None,
                },
            ),
            (
                r#"{"target":"merge","base":"main"}"#,
                RecipeTarget::Merge {
                    base: revision("main"),
                    pinned: None,
                },
            ),
            (
                r#"{"target":"last","count":2}"#,
                RecipeTarget::Last {
                    count: NonZeroU32::new(2).unwrap(),
                    pinned: None,
                },
            ),
        ] {
            let target: RecipeTarget =
                serde_json::from_str(json).expect("legacy target tag remains readable");
            assert_eq!(target, expected);
        }

        for (json, expected) in [
            (
                r#"{"op":"diff","target":{"target":"unpushed"}}"#,
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            ),
            (
                r#"{"op":"merge-diff","base":null}"#,
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
            ),
        ] {
            let op: RecipeOp =
                serde_json::from_str(json).expect("legacy operation tag remains readable");
            assert_eq!(op, expected);
        }

        for (json, expected) in [
            (r#""snapshot""#, RecipeBatchKind::Snapshot),
            (r#""live""#, RecipeBatchKind::Live),
        ] {
            let kind: RecipeBatchKind =
                serde_json::from_str(json).expect("legacy batch tag remains readable");
            assert_eq!(kind, expected);
        }
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
        let json = r#"{"source":{"kind":"GithubRepo","value":"o/r"},"op":{"op":"merge-diff","base":null}}"#;
        assert!(serde_json::from_str::<Recipe>(json).is_err());
    }

    #[test]
    fn token_round_trips_through_encode_and_decode() {
        let batch = sample_batch();
        let token = encode_token(&batch).expect("sample batch encodes");
        assert!(token.starts_with(RECIPE_TOKEN_PREFIX));
        assert_eq!(decode_token(&token), Some(batch));
    }

    #[test]
    fn old_batch_token_without_kind_decodes_as_snapshot() {
        let old_json = r#"{"batch_id":"0198a859-7c4e-7e5f-9e63-ec7bb768d841","recipes":[]}"#;
        let token = format!(
            "{RECIPE_TOKEN_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(old_json.as_bytes())
        );

        let batch = decode_token(&token).expect("old token remains readable");

        assert_eq!(batch.kind, RecipeBatchKind::Snapshot);
        assert_eq!(serde_json::to_string(&batch).unwrap(), old_json);
    }

    #[test]
    fn live_batch_kind_round_trips_through_the_token() {
        let mut batch = sample_batch();
        batch.kind = RecipeBatchKind::Live;

        let token = encode_token(&batch).expect("live batch encodes");
        let decoded = decode_token(&token).expect("live token decodes");

        assert_eq!(decoded, batch);
        assert_eq!(decoded.kind, RecipeBatchKind::Live);
    }

    #[test]
    fn decode_rejects_wrong_prefix() {
        assert_eq!(decode_token("diff://x/y"), None);
    }

    #[test]
    fn decode_rejects_invalid_base64_after_prefix() {
        assert_eq!(decode_token("gtl-recipe://!!!"), None);
    }

    #[test]
    fn decode_rejects_valid_base64_that_is_not_json() {
        let garbage = URL_SAFE_NO_PAD.encode(b"not json");
        assert_eq!(
            decode_token(&format!("{RECIPE_TOKEN_PREFIX}{garbage}")),
            None
        );
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
        let pin = Some(pinned_range("a".repeat(40), "b".repeat(40)));
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
                    pinned: Some(pinned_range("a".repeat(40), head.repeat(40))),
                },
            },
            name: None,
        };
        assert_ne!(recipe_with("b"), recipe_with("c"));
        assert_eq!(recipe_with("b").unpinned(), recipe_with("c").unpinned());
    }
}
