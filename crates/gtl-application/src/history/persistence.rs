//! The persisted recent-render row shape and its [`Recipe`] codec: the single
//! place that maps the recipe model onto the relational columns of
//! `recent_renders` and its `project_sources` / `render_operations` /
//! `render_targets` vocabulary tables.

use std::{num::NonZeroU32, path::PathBuf};

use gtl_contracts::recipes::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget};
use gtl_models::viewer::RenderHistoryId;
#[cfg(test)]
use rusqlite::Connection;

/// The `project_sources.kind` value for a repository addressed by directory.
pub(super) const SOURCE_KIND_DIRECTORY: &str = "directory";

/// One persisted render recipe with its stable row identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRenderRecord {
    pub id: RenderHistoryId,
    pub recipe: Recipe,
    pub title: String,
    pub repo_name: String,
    pub range_label: String,
    pub rendered_at: String,
}

/// The relational projection of one [`Recipe`], ready to bind as SQL
/// parameters. Operation and target names match the seeded
/// `render_operations` / `render_targets` rows.
pub(super) struct RecipeColumns {
    pub(super) source_kind: &'static str,
    pub(super) source_value: String,
    pub(super) operation: &'static str,
    pub(super) target: Option<&'static str>,
    pub(super) argument: Option<String>,
    pub(super) pinned: Option<PinnedRange>,
    pub(super) recipe_name: Option<String>,
}

impl RecipeColumns {
    pub(super) fn from_recipe(recipe: &Recipe) -> Self {
        let RecipeSource::LocalRepo(path) = &recipe.source;
        let (operation, target, argument, pinned) = match &recipe.op {
            RecipeOp::Diff { target } => {
                let (target_name, argument, pinned) = match target {
                    RecipeTarget::Unpushed { pinned } => ("unpushed", None, pinned.clone()),
                    RecipeTarget::Base { rev } => ("base", Some(rev.clone()), None),
                    RecipeTarget::Range { range, pinned } => {
                        ("range", Some(range.clone()), pinned.clone())
                    }
                    RecipeTarget::Merge { base, pinned } => {
                        ("merge", Some(base.clone()), pinned.clone())
                    }
                    RecipeTarget::Last { count, pinned } => {
                        ("last", Some(count.to_string()), pinned.clone())
                    }
                };
                ("diff", Some(target_name), argument, pinned)
            }
            RecipeOp::MergeDiff { base, pinned } => {
                ("merge_diff", None, base.clone(), pinned.clone())
            }
            RecipeOp::SquashPreview { pinned } => ("squash_preview", None, None, pinned.clone()),
        };
        Self {
            source_kind: SOURCE_KIND_DIRECTORY,
            source_value: path.display().to_string(),
            operation,
            target,
            argument,
            pinned,
            recipe_name: recipe.name.clone(),
        }
    }
}

/// One raw joined `recent_renders` row, before identity and recipe validation.
pub(super) struct RecentRenderRow {
    id: i64,
    source_kind: String,
    source_value: String,
    operation: String,
    target: Option<String>,
    argument: Option<String>,
    pinned_base: Option<String>,
    pinned_head: Option<String>,
    recipe_name: Option<String>,
    title: String,
    repo_name: String,
    range_label: String,
    rendered_at: String,
}

/// The column list every recent-render query selects, in
/// [`RecentRenderRow::from_row`] order.
pub(super) const RECENT_RENDER_SELECT: &str = "
SELECT r.id, s.kind, s.value, o.name, t.name, r.argument,
       r.pinned_base, r.pinned_head, r.recipe_name,
       r.title, r.repo_name, r.range_label, r.rendered_at
FROM recent_renders r
JOIN project_sources s ON s.id = r.source_id
JOIN render_operations o ON o.id = r.operation_id
LEFT JOIN render_targets t ON t.id = r.target_id";

impl RecentRenderRow {
    pub(super) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            source_kind: row.get(1)?,
            source_value: row.get(2)?,
            operation: row.get(3)?,
            target: row.get(4)?,
            argument: row.get(5)?,
            pinned_base: row.get(6)?,
            pinned_head: row.get(7)?,
            recipe_name: row.get(8)?,
            title: row.get(9)?,
            repo_name: row.get(10)?,
            range_label: row.get(11)?,
            rendered_at: row.get(12)?,
        })
    }

    pub(super) fn try_into_record(self) -> Result<RecentRenderRecord, RecentRenderRowError> {
        let id = RenderHistoryId::try_new(self.id)
            .map_err(|_| RecentRenderRowError::InvalidId { id: self.id })?;
        let recipe =
            self.decode_recipe()
                .map_err(|reason| RecentRenderRowError::InvalidRecipe {
                    id: self.id,
                    reason,
                })?;
        Ok(RecentRenderRecord {
            id,
            recipe,
            title: self.title,
            repo_name: self.repo_name,
            range_label: self.range_label,
            rendered_at: self.rendered_at,
        })
    }

    fn decode_recipe(&self) -> Result<Recipe, String> {
        let source = match self.source_kind.as_str() {
            SOURCE_KIND_DIRECTORY => RecipeSource::LocalRepo(PathBuf::from(&self.source_value)),
            other => return Err(format!("unknown project-source kind '{other}'")),
        };
        let pinned = match (&self.pinned_base, &self.pinned_head) {
            (Some(base), Some(head)) => Some(PinnedRange {
                base: base.clone(),
                head: head.clone(),
            }),
            (None, None) => None,
            _ => return Err("pinned base and head must be present together".into()),
        };
        let op = match self.operation.as_str() {
            "diff" => RecipeOp::Diff {
                target: self.decode_target(pinned)?,
            },
            "merge_diff" => RecipeOp::MergeDiff {
                base: self.argument.clone(),
                pinned,
            },
            "squash_preview" => RecipeOp::SquashPreview { pinned },
            other => return Err(format!("unknown render operation '{other}'")),
        };
        Ok(Recipe {
            source,
            op,
            name: self.recipe_name.clone(),
        })
    }

    fn decode_target(&self, pinned: Option<PinnedRange>) -> Result<RecipeTarget, String> {
        let target = self.target.as_deref().ok_or("diff row carries no target")?;
        let argument = || {
            self.argument
                .clone()
                .ok_or_else(|| format!("target '{target}' requires an argument"))
        };
        Ok(match target {
            "unpushed" => RecipeTarget::Unpushed { pinned },
            "base" => {
                if pinned.is_some() {
                    return Err("target 'base' does not accept a pin".into());
                }
                RecipeTarget::Base { rev: argument()? }
            }
            "range" => RecipeTarget::Range {
                range: argument()?,
                pinned,
            },
            "merge" => RecipeTarget::Merge {
                base: argument()?,
                pinned,
            },
            "last" => RecipeTarget::Last {
                count: argument()?
                    .parse::<NonZeroU32>()
                    .map_err(|error| format!("target 'last' count is invalid: {error}"))?,
                pinned,
            },
            other => return Err(format!("unknown diff target '{other}'")),
        })
    }
}

/// Reports a persisted `recent_renders` row that fails identity or recipe
/// validation, for the recent-render operations to wrap.
#[derive(Debug, thiserror::Error)]
pub enum RecentRenderRowError {
    #[error("recent_renders row id {id} violates the positive-ID invariant")]
    InvalidId { id: i64 },
    #[error("recent_renders row id {id} holds an undecodable recipe: {reason}")]
    InvalidRecipe { id: i64, reason: String },
}

#[cfg(test)]
pub(crate) fn store_test() -> Connection {
    let connection = Connection::open_in_memory().expect("history test connection");
    connection
        .execute_batch(
            "CREATE TABLE project_sources (
          id         INTEGER PRIMARY KEY AUTOINCREMENT,
          kind       TEXT NOT NULL CHECK (kind IN ('directory', 'remote')),
          value      TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT,
          UNIQUE (kind, value)
        ) STRICT;
        CREATE TABLE render_operations (
          id   INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE
        ) STRICT;
        INSERT INTO render_operations (id, name) VALUES
          (1, 'diff'), (2, 'merge_diff'), (3, 'squash_preview');
        CREATE TABLE render_targets (
          id   INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE
        ) STRICT;
        INSERT INTO render_targets (id, name) VALUES
          (1, 'unpushed'), (2, 'base'), (3, 'range'), (4, 'merge'), (5, 'last');
        CREATE TABLE recent_renders (
          id           INTEGER PRIMARY KEY,
          source_id    INTEGER NOT NULL REFERENCES project_sources (id),
          operation_id INTEGER NOT NULL REFERENCES render_operations (id),
          target_id    INTEGER REFERENCES render_targets (id),
          argument     TEXT,
          pinned_base  TEXT,
          pinned_head  TEXT,
          recipe_name  TEXT,
          title        TEXT NOT NULL,
          repo_name    TEXT NOT NULL,
          range_label  TEXT NOT NULL,
          rendered_at  TEXT NOT NULL,
          CHECK ((pinned_base IS NULL) = (pinned_head IS NULL)),
          CHECK ((operation_id = 1) = (target_id IS NOT NULL))
        ) STRICT;
        CREATE UNIQUE INDEX recent_renders_fingerprint_idx
        ON recent_renders (
          source_id,
          repo_name,
          coalesce(pinned_base, X''),
          coalesce(pinned_head, X'')
        );
        CREATE INDEX recent_renders_repo_name_idx
        ON recent_renders (repo_name);
        CREATE INDEX project_sources_value_idx
        ON project_sources (value);",
        )
        .expect("history test schema");
    connection
}

/// Inserts one minimal unpushed-diff render row under `id`, sharing a single
/// seeded project source across calls.
#[cfg(test)]
pub(crate) fn seed_recent_render(connection: &Connection, id: i64, title: &str) {
    connection
        .execute_batch(&format!(
            "INSERT OR IGNORE INTO project_sources (id, kind, value, created_at) \
             VALUES (7, 'directory', '/repos/gt', '2026-07-11T00:00:00Z');
             INSERT INTO recent_renders \
             (id, source_id, operation_id, target_id, pinned_base, pinned_head, \
              title, repo_name, range_label, rendered_at) \
             VALUES ({id}, 7, 1, 1, 'base-{id}', 'head-{id}', '{title}', \
             'git-tools', 'main..HEAD', '2026-07-11T00:00:00Z');"
        ))
        .expect("seed recent render");
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        history::{list_recent_render_page, record_render},
        testing::FixedClock,
    };

    fn assert_round_trips(recipe: &Recipe) {
        let mut connection = store_test();
        record_render::execute(
            record_render::RecordRender {
                recipe: recipe.clone(),
                title: "t".into(),
                repo_name: "gt".into(),
                range_label: "main..HEAD".into(),
            },
            &mut connection,
            &FixedClock("2026-07-11T00:00:00Z".into()),
        )
        .expect("record succeeds");

        let entries = list_recent_render_page::execute(
            list_recent_render_page::ListRecentRenderPage::default(),
            &connection,
        )
        .expect("list succeeds")
        .entries;

        assert_eq!(entries.len(), 1, "recipe {recipe:?} persists one row");
        assert_eq!(&entries[0].recipe, recipe, "recipe survives the row codec");
    }

    /// The row codec is the persistence contract: every operation and target
    /// shape must survive a record -> list round trip unchanged.
    #[test]
    fn every_recipe_shape_round_trips_through_the_relational_codec() {
        let pin = Some(PinnedRange {
            base: "a".repeat(40),
            head: "b".repeat(40),
        });
        let targets = [
            RecipeTarget::Unpushed {
                pinned: pin.clone(),
            },
            RecipeTarget::Base {
                rev: "HEAD~2".into(),
            },
            RecipeTarget::Range {
                range: "a..b".into(),
                pinned: None,
            },
            RecipeTarget::Merge {
                base: "main".into(),
                pinned: pin.clone(),
            },
            RecipeTarget::Last {
                count: NonZeroU32::new(3).expect("positive count"),
                pinned: None,
            },
        ];
        let ops = targets
            .into_iter()
            .map(|target| RecipeOp::Diff { target })
            .chain([
                RecipeOp::MergeDiff {
                    base: Some("main".into()),
                    pinned: None,
                },
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: pin.clone(),
                },
                RecipeOp::SquashPreview { pinned: pin },
            ]);
        for (index, op) in ops.enumerate() {
            assert_round_trips(&Recipe {
                source: RecipeSource::LocalRepo("/repos/gt".into()),
                op,
                name: (index % 2 == 0).then(|| "named".into()),
            });
        }
    }
}
