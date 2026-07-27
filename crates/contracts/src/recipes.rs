//! Recipe wire values describe *how to produce* a view -
//! source identity + operation — never view data. Crosses the Tauri IPC
//! boundary as JSON; the app-history log persists it relationally through
//! `application::history`.
//! `Recipe::unpinned()` equality is the snapshot-tab dedupe identity (pins
//! differ across runs of the same repo + operation); live recipes are always
//! unpinned, so full `Recipe` equality still governs their identity.
//!
//! This module is deliberately app-agnostic: it holds only pure serde DTOs and
//! the codec, with no dependency on `domain`. [`RecipeTarget`] mirrors
//! `application::diffs::DiffTarget`'s shape by hand; the application viewer
//! slice owns the mapping between them.
//!
//! This module also hosts the argv-token codec Phase 5 uses to hand a batch of
//! recipes from the CLI to the single-instance viewer: [`OpenRecipes`] is a
//! named batch, [`encode_token`]/[`decode_token`] round-trip it through a
//! `gtl-recipe://`-prefixed, base64url-encoded argv string.
//!
//! Every enum serializes its wire tag in `snake_case`. Deserialization also
//! accepts the legacy `PascalCase` source and `kebab-case` operation tags so
//! argv tokens remain readable across the migration.

use std::{num::NonZeroU32, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// The identity of the repository a recipe renders from.
///
/// Recipe JSON uses the `local_repo` tag. The live-view store's separate
/// `source_kind` identity remains the stable `LocalRepo` string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RecipeSource {
    #[serde(alias = "LocalRepo")]
    LocalRepo(PathBuf),
}

/// A commit range resolved to immutable SHAs at invocation time. `None` on a
/// target means "resolve symbolically at compute time" — the live behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedRange {
    /// Full SHA of the range base (exclusive end).
    pub base: String,
    /// Full SHA of the range head (inclusive end).
    pub head: String,
}

/// A hand-maintained serde mirror of `application::diffs::DiffTarget`. This crate
/// carries no `domain` dependency (it must stay app-agnostic); the mapping onto
/// the domain type lives in the consuming crate.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum RecipeTarget {
    Unpushed {
        pinned: Option<PinnedRange>,
    },
    Base {
        rev: String,
    },
    Range {
        range: String,
        pinned: Option<PinnedRange>,
    },
    Merge {
        base: String,
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
        base: Option<String>,
        pinned: Option<PinnedRange>,
    },
    #[serde(alias = "squash-preview")]
    SquashPreview {
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
    pub name: Option<String>,
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
            RecipeOp::SquashPreview { .. } => "squash-preview",
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
            RecipeOp::MergeDiff { pinned, .. } | RecipeOp::SquashPreview { pinned } => {
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

impl RecipeBatchKind {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde skip_serializing_if predicates receive a shared reference"
    )]
    const fn is_snapshot(&self) -> bool {
        matches!(self, Self::Snapshot)
    }
}

/// A homogeneous batch of recipes to open together — the unit the CLI hands to
/// the single-instance viewer as one argv token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRecipes {
    pub batch_id: String,
    /// The viewer behavior shared by every recipe in this batch.
    #[serde(default, skip_serializing_if = "RecipeBatchKind::is_snapshot")]
    pub kind: RecipeBatchKind,
    pub recipes: Vec<Recipe>,
}

/// The argv-token prefix that marks a `gtl-viewer` command-line argument as an
/// encoded [`OpenRecipes`] batch rather than a plain file path.
pub const RECIPE_TOKEN_PREFIX: &str = "gtl-recipe://";

/// Encode a recipe batch into a single self-contained argv token: JSON
/// serialized, then base64url (no padding) encoded, prefixed with
/// [`RECIPE_TOKEN_PREFIX`].
///
/// # Panics
///
/// Panics only if serializing [`OpenRecipes`] to JSON fails — which cannot
/// happen for this data shape (plain strings, paths, and enums; no map with
/// non-string keys and no fallible custom `Serialize`).
pub fn encode_token(batch: &OpenRecipes) -> String {
    let json = serde_json::to_vec(batch).expect("OpenRecipes serializes to JSON infallibly");
    format!("{RECIPE_TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
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

    fn diff_recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn sample_batch() -> OpenRecipes {
        OpenRecipes {
            batch_id: "batch-1".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![
                diff_recipe(),
                Recipe {
                    source: RecipeSource::LocalRepo(PathBuf::from("/repos/other")),
                    op: RecipeOp::SquashPreview { pinned: None },
                    name: None,
                },
            ],
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
    fn canonical_multiword_operation_tags_are_snake_case() {
        for (op, expected_tag) in [
            (
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                "merge_diff",
            ),
            (RecipeOp::SquashPreview { pinned: None }, "squash_preview"),
        ] {
            let recipe = Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
                op,
                name: None,
            };

            let json = serde_json::to_value(recipe).expect("recipe serializes");

            assert_eq!(json["op"]["op"], expected_tag);
        }
    }

    #[test]
    fn every_legacy_recipe_enum_tag_still_deserializes() {
        let source: RecipeSource =
            serde_json::from_str(r#"{"kind":"LocalRepo","value":"/repos/gt"}"#)
                .expect("legacy source tag remains readable");
        assert_eq!(source, RecipeSource::LocalRepo("/repos/gt".into()));

        for (json, expected) in [
            (
                r#"{"target":"unpushed"}"#,
                RecipeTarget::Unpushed { pinned: None },
            ),
            (
                r#"{"target":"base","rev":"HEAD"}"#,
                RecipeTarget::Base { rev: "HEAD".into() },
            ),
            (
                r#"{"target":"range","range":"a..b"}"#,
                RecipeTarget::Range {
                    range: "a..b".into(),
                    pinned: None,
                },
            ),
            (
                r#"{"target":"merge","base":"main"}"#,
                RecipeTarget::Merge {
                    base: "main".into(),
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
            (
                r#"{"op":"squash-preview"}"#,
                RecipeOp::SquashPreview { pinned: None },
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
    fn legacy_recipe_token_tags_still_decode() {
        let legacy_json = r#"{"batch_id":"old","kind":"live","recipes":[{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"squash-preview"}}]}"#;
        let token = format!(
            "{RECIPE_TOKEN_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(legacy_json.as_bytes())
        );

        let batch = decode_token(&token).expect("legacy argv token remains readable");

        assert_eq!(batch.kind, RecipeBatchKind::Live);
        assert_eq!(batch.recipes.len(), 1);
        assert_eq!(
            batch.recipes[0].op,
            RecipeOp::SquashPreview { pinned: None }
        );
    }

    #[test]
    fn recipe_round_trips_through_json() {
        for recipe in [
            diff_recipe(),
            Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
                op: RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                },
                name: None,
            },
            Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
                op: RecipeOp::SquashPreview { pinned: None },
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
            (RecipeOp::SquashPreview { pinned: None }, "squash-preview"),
        ] {
            let recipe = Recipe {
                source: RecipeSource::LocalRepo("/repos/gt".into()),
                op,
                name: None,
            };

            assert_eq!(recipe.kind_tag(), expected);
        }
    }

    #[test]
    fn unknown_source_kind_is_rejected_at_deserialization() {
        let json = r#"{"source":{"kind":"GithubRepo","value":"o/r"},"op":{"op":"squash-preview"}}"#;
        assert!(serde_json::from_str::<Recipe>(json).is_err());
    }

    #[test]
    fn token_round_trips_through_encode_and_decode() {
        let batch = sample_batch();
        let token = encode_token(&batch);
        assert!(token.starts_with(RECIPE_TOKEN_PREFIX));
        assert_eq!(decode_token(&token), Some(batch));
    }

    #[test]
    fn old_batch_token_without_kind_decodes_as_snapshot() {
        let old_json = r#"{"batch_id":"old","recipes":[]}"#;
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

        let decoded = decode_token(&encode_token(&batch)).expect("live token decodes");

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
            source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(PinnedRange {
                        base: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        head: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                    }),
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
        let pin = Some(PinnedRange {
            base: "a".repeat(40),
            head: "b".repeat(40),
        });
        let cases = vec![
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: pin.clone(),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: "x..y".into(),
                    pinned: pin.clone(),
                },
            },
            RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: "main".into(),
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
                base: Some("main".into()),
                pinned: pin.clone(),
            },
            RecipeOp::SquashPreview { pinned: pin },
        ];
        for op in cases {
            let recipe = Recipe {
                source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
                op,
                name: Some("n".into()),
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
            source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(PinnedRange {
                        base: "a".repeat(40),
                        head: head.repeat(40),
                    }),
                },
            },
            name: None,
        };
        assert_ne!(recipe_with("b"), recipe_with("c"));
        assert_eq!(recipe_with("b").unpinned(), recipe_with("c").unpinned());
    }
}
