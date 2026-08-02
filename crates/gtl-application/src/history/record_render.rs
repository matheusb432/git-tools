//! The `history/record_render` vertical slice: record one render in the app history log.

use gtl_contracts::recipes::Recipe;
use rusqlite::{Connection, params};

use crate::{
    history::persistence::RecipeColumns,
    ports::{AppStateStore, Clock},
};

const RECENT_RENDERS_CAP: usize = 500;

/// Record one render in the app history.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordRender {
    pub recipe: Recipe,
    pub title: String,
    pub repo_name: String,
    pub range_label: String,
}

/// Successful result when a render is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRenderOk {}

/// Error when recording a render fails.
#[derive(Debug, thiserror::Error)]
pub enum RecordRenderError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Records a render through the app-state port.
#[expect(
    clippy::needless_pass_by_value,
    reason = "cqrsy requires request-first operations to take requests by value"
)]
#[cqrsy::command]
pub fn execute(
    req: RecordRender,
    store: &impl AppStateStore,
    clock: &impl Clock,
) -> Result<RecordRenderOk, RecordRenderError> {
    let rendered_at = clock.now_iso();
    let mut connection = store.connection_lock()?;
    record_render(&mut connection, &req, &rendered_at)?;
    Ok(RecordRenderOk {})
}

fn record_render(
    connection: &mut Connection,
    request: &RecordRender,
    rendered_at: &str,
) -> anyhow::Result<()> {
    let columns = RecipeColumns::from_recipe(&request.recipe);
    let transaction = connection.transaction()?;
    // The upsert touches updated_at so a source row tracks when a render last
    // used it; created_at keeps the first sighting.
    let source_id: i64 = transaction
        .prepare_cached(
            "INSERT INTO project_sources (kind, value, created_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (kind, value) DO UPDATE SET updated_at = excluded.created_at
             RETURNING id",
        )?
        .query_row(
            params![columns.source_kind, columns.source_value, rendered_at],
            |row| row.get(0),
        )?;
    {
        let mut statement = transaction.prepare_cached(
            "INSERT INTO recent_renders
               (source_id, operation_id, target_id, argument,
                pinned_base, pinned_head, recipe_name,
                title, repo_name, range_label, rendered_at)
             VALUES
               (?1,
                (SELECT id FROM render_operations WHERE name = ?2),
                (SELECT id FROM render_targets WHERE name = ?3),
                ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT DO NOTHING",
        )?;
        statement.execute(params![
            source_id,
            columns.operation,
            columns.target,
            columns.argument,
            columns.pinned.as_ref().map(|pin| pin.base.as_str()),
            columns.pinned.as_ref().map(|pin| pin.head.as_str()),
            columns.recipe_name,
            request.title,
            request.repo_name,
            request.range_label,
            rendered_at,
        ])?;
    }
    {
        let mut statement = transaction.prepare_cached(
            "DELETE FROM recent_renders WHERE id NOT IN
             (SELECT id FROM recent_renders ORDER BY id DESC LIMIT ?1)",
        )?;
        statement.execute(params![i64::try_from(RECENT_RENDERS_CAP)?])?;
    }
    {
        // Pruning can orphan a source; collect it in the same transaction so
        // project_sources never grows past what recent_renders references.
        let mut statement = transaction.prepare_cached(
            "DELETE FROM project_sources WHERE id NOT IN
             (SELECT source_id FROM recent_renders)",
        )?;
        statement.execute([])?;
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use gtl_contracts::recipes::{PinnedRange, RecipeOp, RecipeSource, RecipeTarget};

    use super::*;
    use crate::{
        history::{RecentRenderRecord, list_recent_renders, persistence::store_test},
        testing::FixedClock,
    };

    fn recipe(repo: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(repo.into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn pinned_recipe(repo: &str, base: &str, head: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(repo.into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(PinnedRange {
                        base: base.into(),
                        head: head.into(),
                    }),
                },
            },
            name: None,
        }
    }

    fn command(title: impl Into<String>) -> RecordRender {
        command_for_repo(title, "/repos/gt")
    }

    fn command_for_repo(title: impl Into<String>, repo: &str) -> RecordRender {
        command_for_recipe(title, "gt", recipe(repo))
    }

    fn command_for_recipe(
        title: impl Into<String>,
        repo_name: impl Into<String>,
        recipe: Recipe,
    ) -> RecordRender {
        RecordRender {
            recipe,
            title: title.into(),
            repo_name: repo_name.into(),
            range_label: "origin/main..HEAD".into(),
        }
    }

    fn list_recent(store: &impl AppStateStore) -> Vec<RecentRenderRecord> {
        list_recent_renders::execute(list_recent_renders::ListRecentRenders, store)
            .expect("list succeeds")
            .entries
    }

    fn project_sources(store: &impl AppStateStore) -> Vec<(String, Option<String>)> {
        store
            .connection_lock()
            .expect("connection lock")
            .prepare("SELECT value, updated_at FROM project_sources ORDER BY value")
            .expect("prepare sources")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query sources")
            .collect::<Result<_, _>>()
            .expect("decode sources")
    }

    #[test]
    fn records_a_render_stamped_by_the_clock() {
        let store = store_test();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        execute(command("gt · unpushed"), &store, &clock).expect("record succeeds");

        let renders = list_recent(&store);
        assert_eq!(renders.len(), 1);
        assert_eq!(
            renders[0],
            RecentRenderRecord {
                id: gtl_models::viewer::RenderHistoryId::try_new(1).expect("positive id"),
                recipe: recipe("/repos/gt"),
                title: "gt · unpushed".into(),
                repo_name: "gt".into(),
                range_label: "origin/main..HEAD".into(),
                rendered_at: "2026-07-07T00:00:00Z".into(),
            }
        );
    }

    #[test]
    fn repeated_renders_share_one_touched_project_source() {
        let store = store_test();
        execute(
            command("first"),
            &store,
            &FixedClock("2026-07-07T00:00:00Z".into()),
        )
        .expect("record succeeds");
        execute(
            command("second"),
            &store,
            &FixedClock("2026-07-08T00:00:00Z".into()),
        )
        .expect("record succeeds");

        assert_eq!(
            project_sources(&store),
            vec![("/repos/gt".into(), Some("2026-07-08T00:00:00Z".into()))]
        );
    }

    #[test]
    fn repeated_fingerprint_preserves_the_original_render() {
        let store = store_test();
        let first = command_for_recipe("first", "gt", pinned_recipe("/repos/gt", "base", "head"));
        let mut repeated = first.clone();
        repeated.title = "repeated".into();

        execute(first, &store, &FixedClock("2026-07-07T00:00:00Z".into()))
            .expect("first record succeeds");
        execute(repeated, &store, &FixedClock("2026-07-08T00:00:00Z".into()))
            .expect("repeated fingerprint is a successful no-op");

        let renders = list_recent(&store);
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].title, "first");
        assert_eq!(renders[0].rendered_at, "2026-07-07T00:00:00Z");
    }

    #[test]
    fn every_fingerprint_field_distinguishes_a_render() {
        let store = store_test();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let commands = [
            command_for_recipe("original", "gt", pinned_recipe("/repos/gt", "base", "head")),
            command_for_recipe(
                "source",
                "gt",
                pinned_recipe("/repos/other", "base", "head"),
            ),
            command_for_recipe("repo", "other", pinned_recipe("/repos/gt", "base", "head")),
            command_for_recipe(
                "base",
                "gt",
                pinned_recipe("/repos/gt", "other-base", "head"),
            ),
            command_for_recipe(
                "head",
                "gt",
                pinned_recipe("/repos/gt", "base", "other-head"),
            ),
        ];

        for command in commands {
            execute(command, &store, &clock).expect("distinct record succeeds");
        }

        assert_eq!(list_recent(&store).len(), 5);
    }

    #[test]
    fn recording_past_the_cap_prunes_oldest_rows_and_orphaned_sources() {
        let store = store_test();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());

        // The first five renders come from a repo no later render references,
        // so pruning them must also collect its project_sources row.
        for index in 0..5 {
            execute(
                command_for_recipe(
                    format!("render {index}"),
                    "gt",
                    pinned_recipe(
                        "/repos/old",
                        &format!("base-{index}"),
                        &format!("head-{index}"),
                    ),
                ),
                &store,
                &clock,
            )
            .expect("record succeeds");
        }
        for index in 5..(RECENT_RENDERS_CAP + 5) {
            execute(
                command_for_recipe(
                    format!("render {index}"),
                    "gt",
                    pinned_recipe(
                        "/repos/gt",
                        &format!("base-{index}"),
                        &format!("head-{index}"),
                    ),
                ),
                &store,
                &clock,
            )
            .expect("record succeeds");
        }

        let renders = list_recent(&store);
        assert_eq!(renders.len(), RECENT_RENDERS_CAP);
        assert_eq!(renders[0].title, "render 504");
        assert_eq!(renders[RECENT_RENDERS_CAP - 1].title, "render 5");
        assert_eq!(
            project_sources(&store)
                .into_iter()
                .map(|(value, _)| value)
                .collect::<Vec<_>>(),
            vec!["/repos/gt".to_owned()]
        );
    }
}
