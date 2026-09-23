//! The persisted recent-render row shape and its [`Recipe`] codec: the single
//! place that maps the recipe model onto the relational columns of
//! `recent_renders` and its `render_sources` / `render_operations` /
//! `render_targets` vocabulary tables.

use std::num::NonZeroU32;

use gtl_models::{paths::ProjectName, timestamps::MachineTimestamp, viewer::RenderHistoryId};
#[cfg(test)]
use rusqlite::Connection;

use crate::recipes::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget};

/// The `render_sources.kind` value for a repository addressed by directory.
pub(super) const SOURCE_KIND_DIRECTORY: &str = "directory";

/// One persisted render recipe with its stable row identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRenderRecord {
    pub project_id: Option<gtl_models::projects::catalogue::ProjectId>,
    pub id: RenderHistoryId,
    pub recipe: Recipe,
    pub title: String,
    pub repo_name: ProjectName,
    pub range_label: String,
    pub rendered_at: MachineTimestamp,
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
    pub(super) recipe_name: Option<ProjectName>,
}

impl RecipeColumns {
    pub(super) fn from_recipe(recipe: &Recipe) -> Self {
        let RecipeSource::LocalRepo(path) = &recipe.source;
        let (operation, target, argument, pinned) = match &recipe.op {
            RecipeOp::Diff { target } => {
                let (target_name, argument, pinned) = target_columns(target);
                ("diff", Some(target_name), argument, pinned)
            }
            RecipeOp::MergeDiff { base, pinned } => (
                "merge_diff",
                None,
                base.as_ref().map(ToString::to_string),
                pinned.clone(),
            ),
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

fn target_columns(target: &RecipeTarget) -> (&'static str, Option<String>, Option<PinnedRange>) {
    match target {
        RecipeTarget::Unpushed { pinned } => ("unpushed", None, pinned.clone()),
        RecipeTarget::Base { rev } => ("base", Some(rev.to_string()), None),
        RecipeTarget::Range { range, pinned } => ("range", Some(range.to_string()), pinned.clone()),
        RecipeTarget::Merge { base, pinned } => ("merge", Some(base.to_string()), pinned.clone()),
        RecipeTarget::Last { count, pinned } => ("last", Some(count.to_string()), pinned.clone()),
    }
}

/// One raw joined `recent_renders` row, before identity and recipe validation.
pub(super) struct RecentRenderRow {
    project_id: Option<String>,
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
       r.title, r.repo_name, r.range_label, r.rendered_at, r.project_id
FROM recent_renders r
JOIN render_sources s ON s.id = r.source_id
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
            project_id: row.get(13)?,
        })
    }

    pub(super) fn try_into_record(self) -> Result<RecentRenderRecord, RecentRenderRowError> {
        let id = RenderHistoryId::try_new(self.id)
            .map_err(|_| RecentRenderRowError::Id { id: self.id })?;
        let recipe = self
            .decode_recipe()
            .map_err(|reason| RecentRenderRowError::Recipe {
                id: self.id,
                reason,
            })?;
        let project_id = self
            .project_id
            .map(gtl_models::projects::catalogue::ProjectId::try_new)
            .transpose()
            .map_err(|error| RecentRenderRowError::Project {
                id: self.id,
                reason: error.to_string(),
            })?;
        Ok(RecentRenderRecord {
            project_id,
            id,
            recipe,
            title: self.title,
            repo_name: ProjectName::try_new(self.repo_name).map_err(|error| {
                RecentRenderRowError::ProjectName {
                    id: self.id,
                    field: "repo_name",
                    reason: error.to_string(),
                }
            })?,
            range_label: self.range_label,
            rendered_at: MachineTimestamp::try_from(self.rendered_at).map_err(|error| {
                RecentRenderRowError::Timestamp {
                    id: self.id,
                    reason: error.to_string(),
                }
            })?,
        })
    }

    fn decode_recipe(&self) -> Result<Recipe, String> {
        let source = match self.source_kind.as_str() {
            SOURCE_KIND_DIRECTORY => RecipeSource::LocalRepo(
                gtl_models::paths::RepositoryRoot::try_new(self.source_value.clone().into())
                    .map_err(|error| format!("source repository root is invalid: {error}"))?,
            ),
            other => return Err(format!("unknown project-source kind '{other}'")),
        };
        let pinned = match (&self.pinned_base, &self.pinned_head) {
            (Some(base), Some(head)) => Some(PinnedRange {
                base: base
                    .as_str()
                    .try_into()
                    .map_err(|error| format!("pinned base is invalid: {error}"))?,
                head: head
                    .as_str()
                    .try_into()
                    .map_err(|error| format!("pinned head is invalid: {error}"))?,
            }),
            (None, None) => None,
            _ => return Err("pinned base and head must be present together".into()),
        };
        let op = match self.operation.as_str() {
            "diff" => RecipeOp::Diff {
                target: self.decode_target(pinned)?,
            },
            "merge_diff" => RecipeOp::MergeDiff {
                base: self
                    .argument
                    .clone()
                    .map(gtl_models::git::GitRevision::try_new)
                    .transpose()
                    .map_err(|_| "merge-diff base revision is empty".to_owned())?,
                pinned,
            },
            other => return Err(format!("unknown render operation '{other}'")),
        };
        Ok(Recipe {
            source,
            op,
            name: self
                .recipe_name
                .clone()
                .map(ProjectName::try_new)
                .transpose()
                .map_err(|error| format!("recipe name is invalid: {error}"))?,
        })
    }

    fn decode_target(&self, pinned: Option<PinnedRange>) -> Result<RecipeTarget, String> {
        let target = self.target.as_deref().ok_or("diff row carries no target")?;
        let argument = || {
            self.argument
                .clone()
                .ok_or_else(|| format!("target '{target}' requires an argument"))
        };
        if target == "base" && pinned.is_some() {
            return Err("target 'base' does not accept a pin".into());
        }
        Ok(match target {
            "unpushed" => RecipeTarget::Unpushed { pinned },
            "base" => RecipeTarget::Base {
                rev: gtl_models::git::GitRevision::try_new(argument()?)
                    .map_err(|_| "target 'base' revision is empty".to_owned())?,
            },
            "range" => RecipeTarget::Range {
                range: gtl_models::git::GitRange::try_new(argument()?)
                    .map_err(|_| "target 'range' expression is empty".to_owned())?,
                pinned,
            },
            "merge" => RecipeTarget::Merge {
                base: gtl_models::git::GitRevision::try_new(argument()?)
                    .map_err(|_| "target 'merge' base revision is empty".to_owned())?,
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
    #[error("recent_renders row id {id} has an invalid project: {reason}")]
    Project { id: i64, reason: String },
    #[error("recent_renders row id {id} violates the positive-ID invariant")]
    Id { id: i64 },
    #[error("recent_renders row id {id} holds an undecodable recipe: {reason}")]
    Recipe { id: i64, reason: String },
    #[error("recent_renders row id {id} has invalid {field}: {reason}")]
    ProjectName {
        id: i64,
        field: &'static str,
        reason: String,
    },
    #[error("recent_renders row id {id} has an invalid rendered_at timestamp: {reason}")]
    Timestamp { id: i64, reason: String },
}

#[cfg(test)]
pub(super) fn store_test() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE project_render_recency (source_value TEXT PRIMARY KEY, rendered_at TEXT NOT NULL) STRICT;
        CREATE TABLE render_sources (
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
          (1, 'diff'), (2, 'merge_diff');
        CREATE TABLE render_targets (
          id   INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE
        ) STRICT;
        INSERT INTO render_targets (id, name) VALUES
          (1, 'unpushed'), (2, 'base'), (3, 'range'), (4, 'merge'), (5, 'last');
        CREATE TABLE recent_renders (
          id           INTEGER PRIMARY KEY,
          source_id    INTEGER NOT NULL REFERENCES render_sources (id),
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
          render_status TEXT NOT NULL DEFAULT 'success'
            CHECK (render_status IN ('pending', 'success', 'error')),
          CHECK ((pinned_base IS NULL) = (pinned_head IS NULL)),
          CHECK ((operation_id = 1) = (target_id IS NOT NULL))
        ) STRICT;
        CREATE UNIQUE INDEX recent_renders_fingerprint_idx
        ON recent_renders (
          source_id,
          repo_name,
          coalesce(pinned_base, X''),
          coalesce(pinned_head, X'')
        ) WHERE render_status = 'success';
        CREATE INDEX recent_renders_repo_name_idx
        ON recent_renders (repo_name);
        CREATE INDEX render_sources_value_idx
        ON render_sources (value);
        CREATE TABLE render_errors (
          id INTEGER PRIMARY KEY,
          recent_render_id INTEGER NOT NULL
            REFERENCES recent_renders (id) ON DELETE CASCADE,
          error_code TEXT NOT NULL
            CHECK (error_code IN (
              'repository_directory_not_found',
              'repository_directory_not_git_repository',
              'source_unavailable',
              'render_failed',
              'publication_failed'
            )),
          error_detail TEXT NOT NULL CHECK (length(error_detail) > 0)
        ) STRICT;
        CREATE INDEX render_errors_recent_render_id_idx
        ON render_errors (recent_render_id, id);",
        )
        .unwrap();
    connection.execute_batch(
        "CREATE TABLE project_sources (source_id INTEGER PRIMARY KEY, source_kind TEXT NOT NULL, source_value TEXT NOT NULL UNIQUE) STRICT;
         CREATE TABLE projects (id TEXT PRIMARY KEY, source_id INTEGER NOT NULL REFERENCES project_sources(source_id), title TEXT NOT NULL UNIQUE) STRICT;
         ALTER TABLE recent_renders ADD COLUMN project_id TEXT REFERENCES projects(id);"
    ).unwrap();
    connection
}

/// Inserts one minimal unpushed-diff render row under `id`, sharing a single
/// seeded project source across calls.
#[cfg(test)]
pub(super) fn seed_recent_render(connection: &Connection, id: i64, title: &str) {
    let pinned_base = format!("{id:040x}");
    let pinned_head = format!("{:040x}", id + 1_000);
    connection
        .execute_batch(&format!(
            "INSERT OR IGNORE INTO render_sources (id, kind, value, created_at) \
             VALUES (7, 'directory', '/repos/gt', '2026-07-11T00:00:00Z');
             INSERT INTO recent_renders \
             (id, source_id, operation_id, target_id, pinned_base, pinned_head, \
              title, repo_name, range_label, rendered_at) \
             VALUES ({id}, 7, 1, 1, '{pinned_base}', '{pinned_head}', '{title}', \
             'git-tools', 'main..HEAD', '2026-07-11T00:00:00Z');"
        ))
        .unwrap();
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        history::{list_recent_render_page, record_render},
        utils::FixedClock,
    };

    fn assert_round_trips(recipe: &Recipe) {
        let mut connection = store_test();
        record_render::execute(
            &record_render::RecordRender {
                recipe: recipe.clone(),
                title: "t".into(),
                repo_name: crate::utils::project_name("gt"),
                range_label: "main..HEAD".into(),
            },
            &mut connection,
            &FixedClock::from_raw("2026-07-11T00:00:00Z"),
        )
        .unwrap();

        let entries = list_recent_render_page::execute(
            &list_recent_render_page::ListRecentRenderPage::default(),
            &connection,
        )
        .unwrap()
        .entries;

        assert_eq!(entries.len(), 1, "recipe {recipe:?} persists one row");
        assert_eq!(&entries[0].recipe, recipe, "recipe survives the row codec");
    }

    /// The row codec is the persistence contract: every operation and target
    /// shape must survive a record -> list round trip unchanged.
    #[test]
    fn every_recipe_shape_round_trips_through_the_relational_codec() {
        let pin = Some(crate::utils::pinned_range(&"a".repeat(40), &"b".repeat(40)));
        let targets = [
            RecipeTarget::Unpushed {
                pinned: pin.clone(),
            },
            RecipeTarget::Base {
                rev: crate::utils::git_revision("HEAD~2"),
            },
            RecipeTarget::Range {
                range: crate::utils::git_range("a..b"),
                pinned: None,
            },
            RecipeTarget::Merge {
                base: crate::utils::git_revision("main"),
                pinned: pin.clone(),
            },
            RecipeTarget::Last {
                count: NonZeroU32::new(3).unwrap(),
                pinned: None,
            },
        ];
        let ops = targets
            .into_iter()
            .map(|target| RecipeOp::Diff { target })
            .chain([
                RecipeOp::MergeDiff {
                    base: Some(crate::utils::git_revision("main")),
                    pinned: None,
                },
                RecipeOp::MergeDiff {
                    base: None,
                    pinned: pin.clone(),
                },
            ]);
        for (index, op) in ops.enumerate() {
            assert_round_trips(&Recipe {
                source: RecipeSource::LocalRepo(crate::utils::repository_root("/repos/gt")),
                op,
                name: (index % 2 == 0).then(|| crate::utils::project_name("named")),
            });
        }
    }
}
