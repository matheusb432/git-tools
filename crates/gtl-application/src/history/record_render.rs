//! The `history/record_render` vertical slice: record one render in the app history log.

use gtl_models::{paths::ProjectName, timestamps::MachineTimestamp};
use rusqlite::{Connection, params};

use crate::{
    history::persistence::RecipeColumns, ports::Clock, projects::record_project_render,
    recipes::Recipe,
};

const RECENT_RENDERS_CAP: usize = 500;

/// Record one render in the app history.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordRender {
    pub recipe: Recipe,
    pub title: String,
    pub repo_name: ProjectName,
    pub range_label: String,
}

/// Error when recording a render fails.
#[derive(Debug, thiserror::Error)]
pub enum RecordRenderError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Records a render through the application database connection.
pub fn execute(
    req: &RecordRender,
    connection: &mut Connection,
    clock: &impl Clock,
) -> Result<(), RecordRenderError> {
    let rendered_at = clock.now().map_err(anyhow::Error::from)?;
    record_render(connection, req, &rendered_at)?;
    Ok(())
}

fn record_render(
    connection: &mut Connection,
    request: &RecordRender,
    rendered_at: &MachineTimestamp,
) -> anyhow::Result<()> {
    let columns = RecipeColumns::from_recipe(&request.recipe);
    let transaction = connection.transaction()?;
    // The upsert touches updated_at so a source row tracks when a render last
    // used it; created_at keeps the first sighting.
    let source_id: i64 = transaction
        .prepare_cached(
            "INSERT INTO render_sources (kind, value, created_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (kind, value) DO UPDATE SET updated_at = excluded.created_at
             RETURNING id",
        )?
        .query_row(
            params![
                columns.source_kind,
                columns.source_value,
                rendered_at.as_ref()
            ],
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
            columns.pinned.as_ref().map(|pin| pin.base.as_ref()),
            columns.pinned.as_ref().map(|pin| pin.head.as_ref()),
            columns.recipe_name.as_ref().map(|name| name.as_str()),
            request.title,
            request.repo_name.as_str(),
            request.range_label,
            rendered_at.as_ref(),
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
        // render_sources never grows past what recent_renders references.
        let mut statement = transaction.prepare_cached(
            "DELETE FROM render_sources WHERE id NOT IN
             (SELECT source_id FROM recent_renders)",
        )?;
        statement.execute([])?;
    }
    let crate::recipes::RecipeSource::LocalRepo(path) = &request.recipe.source;
    record_project_render::execute(
        &crate::projects::record_project_render::RecordProjectRender {
            path: path.clone(),
            rendered_at: rendered_at.clone(),
        },
        &transaction,
    )?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::{
            RecentRenderRecord, list_recent_render_page, persistence::store_test, record_render,
        },
        recipes::{RecipeOp, RecipeSource, RecipeTarget},
        utils::FixedClock,
    };

    fn recipe(repo: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(crate::utils::repository_root(repo)),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn pinned_recipe(repo: &str, base: &str, head: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(crate::utils::repository_root(repo)),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(crate::utils::pinned_range(base, head)),
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
        repo_name: &str,
        recipe: Recipe,
    ) -> RecordRender {
        RecordRender {
            recipe,
            title: title.into(),
            repo_name: crate::utils::project_name(repo_name),
            range_label: "origin/main..HEAD".into(),
        }
    }

    fn list_recent(connection: &Connection) -> Vec<RecentRenderRecord> {
        list_recent_render_page::execute(
            list_recent_render_page::ListRecentRenderPage::default(),
            connection,
        )
        .unwrap()
        .entries
    }

    fn render_sources(connection: &Connection) -> Vec<(String, Option<String>)> {
        connection
            .prepare("SELECT value, updated_at FROM render_sources ORDER BY value")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn records_a_render_stamped_by_the_clock() {
        let mut connection = store_test();
        let clock = FixedClock::from_raw("2026-07-07T00:00:00Z");
        record_render::execute(&command("gt · unpushed"), &mut connection, &clock).unwrap();

        let renders = list_recent(&connection);
        assert_eq!(renders.len(), 1);
        assert_eq!(
            renders[0],
            RecentRenderRecord {
                id: gtl_models::viewer::RenderHistoryId::try_new(1).unwrap(),
                recipe: recipe("/repos/gt"),
                title: "gt · unpushed".into(),
                repo_name: crate::utils::project_name("gt"),
                range_label: "origin/main..HEAD".into(),
                rendered_at: gtl_models::timestamps::MachineTimestamp::try_from(
                    "2026-07-07T00:00:00Z",
                )
                .unwrap(),
            }
        );
    }

    #[test]
    fn repeated_renders_share_one_touched_project_source() {
        let mut connection = store_test();
        record_render::execute(
            &command("first"),
            &mut connection,
            &FixedClock::from_raw("2026-07-07T00:00:00Z"),
        )
        .unwrap();
        record_render::execute(
            &command("second"),
            &mut connection,
            &FixedClock::from_raw("2026-07-08T00:00:00Z"),
        )
        .unwrap();

        assert_eq!(
            render_sources(&connection),
            vec![("/repos/gt".into(), Some("2026-07-08T00:00:00Z".into()))]
        );
    }

    #[test]
    fn repeated_fingerprint_preserves_the_original_render() {
        let mut connection = store_test();
        let first = command_for_recipe("first", "gt", pinned_recipe("/repos/gt", "base", "head"));
        let mut repeated = first.clone();
        repeated.title = "repeated".into();

        record_render::execute(
            &first,
            &mut connection,
            &FixedClock::from_raw("2026-07-07T00:00:00Z"),
        )
        .unwrap();
        record_render::execute(
            &repeated,
            &mut connection,
            &FixedClock::from_raw("2026-07-08T00:00:00Z"),
        )
        .unwrap();

        let renders = list_recent(&connection);
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].title, "first");
        assert_eq!(renders[0].rendered_at.as_ref(), "2026-07-07T00:00:00Z");
    }

    #[test]
    fn every_fingerprint_field_distinguishes_a_render() {
        let mut connection = store_test();
        let clock = FixedClock::from_raw("2026-07-07T00:00:00Z");
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
            record_render::execute(&command, &mut connection, &clock).unwrap();
        }

        assert_eq!(list_recent(&connection).len(), 5);
    }

    #[test]
    fn recording_past_the_cap_prunes_oldest_rows_and_orphaned_sources() {
        let mut connection = store_test();
        let clock = FixedClock::from_raw("2026-07-07T00:00:00Z");

        // The first five renders come from a repo no later render references,
        // so pruning them must also collect its render_sources row.
        for index in 0..5 {
            record_render::execute(
                &command_for_recipe(
                    format!("render {index}"),
                    "gt",
                    pinned_recipe(
                        "/repos/old",
                        &format!("base-{index}"),
                        &format!("head-{index}"),
                    ),
                ),
                &mut connection,
                &clock,
            )
            .unwrap();
        }
        for index in 5..(RECENT_RENDERS_CAP + 5) {
            record_render::execute(
                &command_for_recipe(
                    format!("render {index}"),
                    "gt",
                    pinned_recipe(
                        "/repos/gt",
                        &format!("base-{index}"),
                        &format!("head-{index}"),
                    ),
                ),
                &mut connection,
                &clock,
            )
            .unwrap();
        }

        let (render_count, newest_title, oldest_title): (i64, String, String) = connection
            .query_row(
                "SELECT COUNT(*),
                        (SELECT title FROM recent_renders ORDER BY id DESC LIMIT 1),
                        (SELECT title FROM recent_renders ORDER BY id ASC LIMIT 1)
                 FROM recent_renders",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(render_count, i64::try_from(RECENT_RENDERS_CAP).unwrap());
        assert_eq!(newest_title, "render 504");
        assert_eq!(oldest_title, "render 5");
        assert_eq!(
            render_sources(&connection)
                .into_iter()
                .map(|(value, _)| value)
                .collect::<Vec<_>>(),
            vec!["/repos/gt".to_owned()]
        );
    }
}
