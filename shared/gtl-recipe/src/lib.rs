//! The recipe DTO: a serializable descriptor of *how to produce* a view —
//! source identity + operation — never view data. Crosses the Tauri IPC
//! boundary and, serialized, the app-history log (`recent_renders.recipe_json`).
//! `Recipe::unpinned()` equality is the snapshot-tab dedupe identity (pins
//! differ across runs of the same repo + operation); live recipes are always
//! unpinned, so full `Recipe` equality still governs their identity.
//!
//! This crate is deliberately app-agnostic: it holds only pure serde DTOs and
//! the codec, with no dependency on `domain`. [`RecipeTarget`] mirrors
//! `domain::diffs::DiffTarget`'s shape by hand; consumers that need the domain
//! type map it locally (the mapping lives in the crate that owns both types —
//! e.g. `crates/desktop`).
//!
//! This crate also hosts the argv-token codec Phase 5 uses to hand a batch of
//! recipes from the CLI to the single-instance viewer: [`OpenRecipes`] is a
//! named batch, [`encode_token`]/[`decode_token`] round-trip it through a
//! `gtl-recipe://`-prefixed, base64url-encoded argv string.

use std::{num::NonZeroU32, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// The identity of the repository a recipe renders from (the live-view source
/// model: `kind` + `value`; only `LocalRepo` exists today).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RecipeSource {
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

/// A hand-maintained serde mirror of `domain::diffs::DiffTarget`. This crate
/// carries no `domain` dependency (it must stay app-agnostic); the mapping onto
/// the domain type lives in the consuming crate.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "kebab-case")]
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
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum RecipeOp {
    Diff {
        target: RecipeTarget,
    },
    MergeDiff {
        base: Option<String>,
        pinned: Option<PinnedRange>,
    },
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
#[serde(rename_all = "kebab-case")]
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
    fn old_recipe_json_without_name_still_deserializes() {
        let json = r#"{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed"}}}"#;

        let recipe: Recipe = serde_json::from_str(json).expect("old history remains readable");

        assert_eq!(recipe.name, None);
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
        assert_eq!(diff_recipe().kind_tag(), "diff");
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
            r#"{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"diff","target":{"target":"unpushed","pinned":{"base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}}"#
        );
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(back, recipe);
    }

    #[test]
    fn old_squash_preview_op_json_still_deserializes_unpinned() {
        let json =
            r#"{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"squash-preview"}}"#;
        let recipe: Recipe = serde_json::from_str(json).expect("old history remains readable");
        assert_eq!(recipe.op, RecipeOp::SquashPreview { pinned: None });
        // And it round-trips back to the identical string (skip_serializing_if).
        assert_eq!(serde_json::to_string(&recipe).unwrap(), json);
    }

    #[test]
    fn old_merge_diff_op_json_still_deserializes_unpinned() {
        let json = r#"{"source":{"kind":"LocalRepo","value":"/repos/gt"},"op":{"op":"merge-diff","base":null}}"#;
        let recipe: Recipe = serde_json::from_str(json).expect("old history remains readable");
        assert_eq!(
            recipe.op,
            RecipeOp::MergeDiff {
                base: None,
                pinned: None
            }
        );
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
