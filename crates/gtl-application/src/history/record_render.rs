//! The `history/record_render` vertical slice: record one render in the app history log.

use std::error::Error;

use anyhow::Context as _;
use gtl_models::{paths::ProjectName, timestamps::MachineTimestamp, viewer::RenderHistoryId};
use rusqlite::{Connection, OptionalExtension as _, Transaction, params};

use crate::{
    history::persistence::RecipeColumns, ports::Clock, projects::record_project_render,
    recipes::Recipe, viewer::initial_recipe_label,
};

const RECENT_RENDERS_CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderStatus {
    Pending,
    Success,
    Error,
}

impl RenderStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Error => "error",
        }
    }
}

impl TryFrom<String> for RenderStatus {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, anyhow::Error> {
        match value.as_str() {
            "pending" => Ok(Self::Pending),
            "success" => Ok(Self::Success),
            "error" => Ok(Self::Error),
            _ => anyhow::bail!("unknown render status '{value}'"),
        }
    }
}

/// Classifies persisted render diagnostics without parsing their detail text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderErrorCode {
    RepositoryDirectoryNotFound,
    RepositoryDirectoryNotGitRepository,
    SourceUnavailable,
    RenderFailed,
    PublicationFailed,
}

impl RenderErrorCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryDirectoryNotFound => "repository_directory_not_found",
            Self::RepositoryDirectoryNotGitRepository => "repository_directory_not_git_repository",
            Self::SourceUnavailable => "source_unavailable",
            Self::RenderFailed => "render_failed",
            Self::PublicationFailed => "publication_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderError {
    code: RenderErrorCode,
    detail: String,
}

/// A non-empty collection of detailed errors for one failed render attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFailure {
    primary: RenderError,
    related: Vec<RenderError>,
}

impl RenderFailure {
    /// Creates a failure with one classified diagnostic.
    #[must_use]
    pub fn new(code: RenderErrorCode, detail: impl Into<String>) -> Self {
        Self {
            primary: RenderError {
                code,
                detail: error_detail(detail.into()),
            },
            related: Vec::new(),
        }
    }

    /// Creates a failure from an error and each source in its causal chain.
    #[must_use]
    pub fn from_error(code: RenderErrorCode, error: &(dyn Error + 'static)) -> Self {
        let mut failure = Self::new(code, error.to_string());
        let mut source = error.source();
        while let Some(error) = source {
            failure.related.push(RenderError {
                code,
                detail: error_detail(error.to_string()),
            });
            source = error.source();
        }
        failure
    }

    fn errors(&self) -> impl Iterator<Item = &RenderError> {
        std::iter::once(&self.primary).chain(&self.related)
    }
}

fn error_detail(detail: String) -> String {
    if detail.is_empty() {
        "no additional error detail".to_owned()
    } else {
        detail
    }
}

/// Starts a render attempt before its background computation begins.
#[derive(Debug, Clone, PartialEq)]
pub struct StartRender {
    recipe: Recipe,
}

impl StartRender {
    /// Creates a request for a new pending attempt.
    #[must_use]
    pub fn new(recipe: Recipe) -> Self {
        Self { recipe }
    }
}

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

impl From<rusqlite::Error> for RecordRenderError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Unexpected(error.into())
    }
}

/// Creates a pending render attempt through the application database connection.
///
/// # Errors
///
/// Returns an error when the clock, database write, or generated identity fails.
pub fn start(
    request: &StartRender,
    connection: &mut Connection,
    clock: &impl Clock,
) -> Result<RenderHistoryId, RecordRenderError> {
    let rendered_at = clock.now().map_err(anyhow::Error::from)?;
    start_render(connection, request, &rendered_at).map_err(Into::into)
}

fn start_render(
    connection: &mut Connection,
    request: &StartRender,
    rendered_at: &MachineTimestamp,
) -> anyhow::Result<RenderHistoryId> {
    let columns = RecipeColumns::from_recipe(&request.recipe);
    let transaction = connection.transaction()?;
    let source_id = touch_render_source(&transaction, &columns, rendered_at)?;
    let title = initial_recipe_label::execute(&request.recipe);
    let repo_name = request.recipe.cwd().project_name();
    let render_id: i64 = transaction
        .prepare_cached(
            "INSERT INTO recent_renders
               (source_id, operation_id, target_id, argument,
                pinned_base, pinned_head, recipe_name,
                title, repo_name, range_label, rendered_at, render_status)
             VALUES
               (?1,
                (SELECT id FROM render_operations WHERE name = ?2),
                (SELECT id FROM render_targets WHERE name = ?3),
                ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             RETURNING id",
        )?
        .query_row(
            params![
                source_id,
                columns.operation,
                columns.target,
                columns.argument,
                columns.pinned.as_ref().map(|pin| pin.base.as_ref()),
                columns.pinned.as_ref().map(|pin| pin.head.as_ref()),
                columns.recipe_name.as_ref().map(|name| name.as_str()),
                title,
                repo_name.as_str(),
                pending_range_label(&request.recipe),
                rendered_at.as_ref(),
                RenderStatus::Pending.as_str(),
            ],
            |row| row.get(0),
        )?;
    prune_recent_renders(&transaction)?;
    transaction.commit()?;
    RenderHistoryId::try_new(render_id).map_err(anyhow::Error::from)
}

fn touch_render_source(
    transaction: &Transaction<'_>,
    columns: &RecipeColumns,
    rendered_at: &MachineTimestamp,
) -> anyhow::Result<i64> {
    // The upsert touches updated_at so a source row tracks when a render last
    // used it; created_at keeps the first sighting.
    transaction
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
        )
        .map_err(Into::into)
}

fn prune_recent_renders(transaction: &Transaction<'_>) -> anyhow::Result<()> {
    transaction.execute(
        "DELETE FROM recent_renders WHERE id NOT IN
         (SELECT id FROM recent_renders ORDER BY id DESC LIMIT ?1)",
        params![i64::try_from(RECENT_RENDERS_CAP)?],
    )?;
    // Pruning can orphan a source; collect it in the same transaction so
    // render_sources never grows past what recent_renders references.
    collect_orphaned_render_sources(transaction)?;
    Ok(())
}

fn collect_orphaned_render_sources(transaction: &Transaction<'_>) -> anyhow::Result<()> {
    transaction.execute(
        "DELETE FROM render_sources WHERE id NOT IN
         (SELECT source_id FROM recent_renders)",
        [],
    )?;
    Ok(())
}

fn pending_range_label(recipe: &Recipe) -> String {
    match &recipe.op {
        crate::recipes::RecipeOp::Diff { target } => match target {
            crate::recipes::RecipeTarget::Unpushed { .. } => "unpushed".to_owned(),
            crate::recipes::RecipeTarget::Base { rev } => format!("{rev}->working"),
            crate::recipes::RecipeTarget::Range { range, .. } => range.to_string(),
            crate::recipes::RecipeTarget::Merge { base, .. } => format!("{base}->HEAD"),
            crate::recipes::RecipeTarget::Last { count, .. } => format!("last {count}"),
        },
        crate::recipes::RecipeOp::MergeDiff { base, .. } => base
            .as_ref()
            .map_or_else(|| "main->HEAD".to_owned(), |base| format!("{base}->HEAD")),
    }
}

/// Changes a pending attempt to success and writes its computed metadata.
///
/// # Errors
///
/// Returns an error when the attempt is absent, no longer pending, or cannot be persisted.
pub fn succeed(
    render_id: RenderHistoryId,
    request: &RecordRender,
    connection: &mut Connection,
) -> Result<RenderHistoryId, RecordRenderError> {
    let columns = RecipeColumns::from_recipe(&request.recipe);
    let transaction = connection.transaction()?;
    let rendered_at = pending_rendered_at(&transaction, render_id)?;
    let source_id = touch_render_source(&transaction, &columns, &rendered_at)?;
    let duplicate_id = transaction
        .query_row(
            "SELECT id FROM recent_renders
             WHERE id != ?1
               AND render_status = 'success'
               AND source_id = ?2
               AND repo_name = ?3
               AND pinned_base IS ?4
               AND pinned_head IS ?5",
            params![
                i64::from(render_id),
                source_id,
                request.repo_name.as_str(),
                columns.pinned.as_ref().map(|pin| pin.base.as_ref()),
                columns.pinned.as_ref().map(|pin| pin.head.as_ref()),
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if duplicate_id.is_some() {
        transaction.execute(
            "DELETE FROM recent_renders WHERE id = ?1 AND render_status = 'pending'",
            [i64::from(render_id)],
        )?;
    } else {
        let updated = transaction.execute(
            "UPDATE recent_renders
             SET source_id = ?2,
                 operation_id = (SELECT id FROM render_operations WHERE name = ?3),
                 target_id = (SELECT id FROM render_targets WHERE name = ?4),
                 argument = ?5,
                 pinned_base = ?6,
                 pinned_head = ?7,
                 recipe_name = ?8,
                 title = ?9,
                 repo_name = ?10,
                 range_label = ?11,
                 render_status = ?12
             WHERE id = ?1 AND render_status = 'pending'",
            params![
                i64::from(render_id),
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
                RenderStatus::Success.as_str(),
            ],
        )?;
        if updated != 1 {
            return Err(anyhow::anyhow!("pending render {render_id} was not updated").into());
        }
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
    duplicate_id.map_or(Ok(render_id), |id| {
        RenderHistoryId::try_new(id).map_err(|error| RecordRenderError::Unexpected(error.into()))
    })
}

fn pending_rendered_at(
    transaction: &Transaction<'_>,
    render_id: RenderHistoryId,
) -> anyhow::Result<MachineTimestamp> {
    let row = transaction
        .query_row(
            "SELECT render_status, rendered_at FROM recent_renders WHERE id = ?1",
            [i64::from(render_id)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .with_context(|| format!("recent render {render_id} is unavailable"))?;
    let status = RenderStatus::try_from(row.0)?;
    anyhow::ensure!(
        status == RenderStatus::Pending,
        "recent render {render_id} is {} instead of pending",
        status.as_str()
    );
    MachineTimestamp::try_from(row.1).map_err(anyhow::Error::from)
}

/// Changes a pending attempt to error and stores its non-empty diagnostic chain.
///
/// # Errors
///
/// Returns an error when the attempt is absent, no longer pending, or cannot be persisted.
pub fn fail(
    render_id: RenderHistoryId,
    failure: &RenderFailure,
    connection: &mut Connection,
) -> Result<(), RecordRenderError> {
    let transaction = connection.transaction()?;
    pending_rendered_at(&transaction, render_id)?;
    let updated = transaction.execute(
        "UPDATE recent_renders SET render_status = ?2
         WHERE id = ?1 AND render_status = 'pending'",
        params![i64::from(render_id), RenderStatus::Error.as_str()],
    )?;
    if updated != 1 {
        return Err(anyhow::anyhow!("pending render {render_id} was not updated").into());
    }
    let mut statement = transaction.prepare_cached(
        "INSERT INTO render_errors (recent_render_id, error_code, error_detail)
         VALUES (?1, ?2, ?3)",
    )?;
    for error in failure.errors() {
        statement.execute(params![
            i64::from(render_id),
            error.code.as_str(),
            &error.detail,
        ])?;
    }
    drop(statement);
    transaction.commit()?;
    Ok(())
}

/// Removes a pending attempt that became stale or intentionally produced no render.
///
/// # Errors
///
/// Returns an error when the attempt is absent, no longer pending, or cannot be removed.
pub fn discard(
    render_id: RenderHistoryId,
    connection: &mut Connection,
) -> Result<(), RecordRenderError> {
    let transaction = connection.transaction()?;
    let deleted = transaction.execute(
        "DELETE FROM recent_renders WHERE id = ?1 AND render_status = 'pending'",
        [i64::from(render_id)],
    )?;
    if deleted != 1 {
        return Err(anyhow::anyhow!("pending render {render_id} was not removed").into());
    }
    collect_orphaned_render_sources(&transaction)?;
    transaction.commit()?;
    Ok(())
}

/// Records an already-computed successful render through the full lifecycle.
///
/// # Errors
///
/// Returns an error when starting or completing the render attempt fails.
pub fn execute(
    request: &RecordRender,
    connection: &mut Connection,
    clock: &impl Clock,
) -> Result<(), RecordRenderError> {
    let render_id = start(&StartRender::new(request.recipe.clone()), connection, clock)?;
    succeed(render_id, request, connection).map(|_| ())
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
            &list_recent_render_page::ListRecentRenderPage::default(),
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

    fn render_status(connection: &Connection, id: RenderHistoryId) -> String {
        connection
            .query_row(
                "SELECT render_status FROM recent_renders WHERE id = ?1",
                [i64::from(id)],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn pending_render_transitions_to_success_without_changing_identity() {
        let mut connection = store_test();
        let command = command("computed title");
        let render_id = record_render::start(
            &StartRender::new(command.recipe.clone()),
            &mut connection,
            &FixedClock::from_raw("2026-07-07T00:00:00Z"),
        )
        .unwrap();

        assert_eq!(render_status(&connection, render_id), "pending");
        assert!(list_recent(&connection).is_empty());

        record_render::succeed(render_id, &command, &mut connection).unwrap();

        assert_eq!(render_status(&connection, render_id), "success");
        let renders = list_recent(&connection);
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].id, render_id);
        assert_eq!(renders[0].title, "computed title");
    }

    #[derive(Debug, thiserror::Error)]
    #[error("render preparation failed")]
    struct RenderTestError {
        #[source]
        source: std::io::Error,
    }

    #[test]
    fn pending_render_transitions_to_error_with_classified_source_details() {
        let mut connection = store_test();
        let render_id = record_render::start(
            &StartRender::new(recipe("/repos/gt")),
            &mut connection,
            &FixedClock::from_raw("2026-07-07T00:00:00Z"),
        )
        .unwrap();
        let error = RenderTestError {
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "repository disappeared"),
        };
        let failure = RenderFailure::from_error(RenderErrorCode::RenderFailed, &error);

        record_render::fail(render_id, &failure, &mut connection).unwrap();

        assert_eq!(render_status(&connection, render_id), "error");
        let errors = connection
            .prepare(
                "SELECT error_code, error_detail FROM render_errors
                 WHERE recent_render_id = ?1 ORDER BY id",
            )
            .unwrap()
            .query_map([i64::from(render_id)], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            errors,
            [
                (
                    "render_failed".to_owned(),
                    "render preparation failed".to_owned()
                ),
                (
                    "render_failed".to_owned(),
                    "repository disappeared".to_owned()
                ),
            ]
        );
        assert!(list_recent(&connection).is_empty());
    }

    #[test]
    fn discarding_pending_render_collects_its_orphaned_source() {
        let mut connection = store_test();
        let render_id = record_render::start(
            &StartRender::new(recipe("/repos/abandoned")),
            &mut connection,
            &FixedClock::from_raw("2026-07-07T00:00:00Z"),
        )
        .unwrap();

        record_render::discard(render_id, &mut connection).unwrap();

        let render_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM recent_renders", [], |row| row.get(0))
            .unwrap();
        let source_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM render_sources", [], |row| row.get(0))
            .unwrap();
        assert_eq!((render_count, source_count), (0, 0));
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
                project_id: None,
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
