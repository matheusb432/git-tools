use gtl_models::{
    failure::{ErrorMeta, Failure, Resource, ViewerFailure},
    paths::ProjectName,
};
use gtl_wire::viewer::RenameViewerSnapshot;
use rusqlite::{Connection, params};

use super::{ViewerState, ViewerStateError, ViewerTabKind};

/// Longest accepted snapshot name, in Unicode scalar values.
pub const SNAPSHOT_NAME_CHARACTERS_MAX: u32 = 200;

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum RenameSnapshotError {
    #[error(
        "snapshot name must contain 1 to {} characters on a single line",
        SNAPSHOT_NAME_CHARACTERS_MAX
    )]
    #[meta(failure = ViewerFailure::SnapshotNameInvalid { characters_max: SNAPSHOT_NAME_CHARACTERS_MAX })]
    InvalidName,
    #[error("viewer tab is not available")]
    #[meta(failure = Failure::Gone { resource: Resource::ViewerTab })]
    UnknownTab,
    #[error("only snapshots can be renamed")]
    #[meta(failure = ViewerFailure::NotSnapshot)]
    NotSnapshot,
    #[error("wait for the snapshot to be saved before renaming it")]
    #[meta(failure = ViewerFailure::SnapshotPending)]
    NotSaved,
    #[error("the saved snapshot is no longer available")]
    #[meta(failure = Failure::Gone { resource: Resource::Snapshot })]
    HistoryMissing,
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    request: &RenameViewerSnapshot,
    state: &ViewerState,
    connection: &Connection,
) -> Result<(), RenameSnapshotError> {
    let text = request.name.trim();
    if text.chars().count() > SNAPSHOT_NAME_CHARACTERS_MAX as usize
        || text.chars().any(char::is_control)
    {
        return Err(RenameSnapshotError::InvalidName);
    }
    let name =
        ProjectName::try_new(text.to_owned()).map_err(|_| RenameSnapshotError::InvalidName)?;
    state.update(|session| {
        let tab = session.tab(request.tab_id).ok_or(RenameSnapshotError::UnknownTab)?;
        if tab.tab.kind() != ViewerTabKind::Snapshot {
            return Err(RenameSnapshotError::NotSnapshot);
        }
        let history_id = tab.history_id.ok_or(RenameSnapshotError::NotSaved)?;
        let changed = connection.execute(
            "UPDATE recent_renders SET recipe_name = ?1 WHERE id = ?2 AND render_status = 'success'",
            params![name.as_str(), i64::from(history_id)],
        ).map_err(anyhow::Error::from)?;
        if changed != 1 {
            return Err(RenameSnapshotError::HistoryMissing);
        }
        session.rename_snapshot(history_id, &name);
        Ok(())
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::CommitCount,
        recipes::{RecipeBatchId, RecipeLabel, RecipeLabelChanges},
        viewer::RenderHistoryId,
    };

    use super::*;
    use crate::{
        history::RecentRenderRecord,
        recipes::{RecipeLabelParts, RecipeOp, RecipeTarget},
        utils,
        viewer::{rename_snapshot, session::CachedView, work},
    };

    fn named(name: &str) -> RecipeLabel {
        RecipeLabel::Named {
            name: utils::project_name(name),
        }
    }

    fn two_commits() -> RecipeLabel {
        RecipeLabel::Changes {
            repository: utils::project_name("project"),
            changes: RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(2),
            },
        }
    }

    fn fixture(kind: ViewerTabKind) -> (ViewerState, Connection, gtl_models::viewer::ViewerTabId) {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE recent_renders (id INTEGER PRIMARY KEY, recipe_name TEXT, render_status TEXT);
            INSERT INTO recent_renders VALUES (1, NULL, 'success');").unwrap();
        let state = ViewerState::new();
        let recipe = utils::viewer::recipe(RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        });
        let work =
            work::reserve_open(&state, recipe.clone(), RecipeBatchId::generate(), kind).unwrap();
        let ticket = work.ticket();
        state
            .update(|session| {
                session.publish_labeled_if_current(
                    ticket,
                    CachedView::new(std::sync::Arc::new(utils::viewer::empty_view())),
                    two_commits(),
                );
                session.bind_snapshot_history(
                    ticket,
                    &RecentRenderRecord {
                        id: RenderHistoryId::try_new(1).unwrap(),
                        project_id: None,
                        recipe,
                        repo_name: utils::project_name("project"),
                        label_parts: RecipeLabelParts::UnpushedCommits {
                            count: CommitCount::new(2),
                        },
                        range_label: "main..HEAD".into(),
                        rendered_at: "2026-09-20T00:00:00Z".parse().unwrap(),
                    },
                );
            })
            .unwrap();
        (state, connection, ticket.tab_id)
    }

    #[test]
    fn stored_names_fill_defaults_without_overriding_explicit_open_names() {
        for (explicit, expected) in [(None, "Saved review"), (Some("CLI review"), "CLI review")] {
            let state = ViewerState::new();
            let mut recipe = utils::viewer::recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            });
            recipe.name = explicit.map(utils::project_name);
            let work = work::reserve_open(
                &state,
                recipe.clone(),
                RecipeBatchId::generate(),
                ViewerTabKind::Snapshot,
            )
            .unwrap();
            recipe.name = Some(utils::project_name("Saved review"));
            let record = RecentRenderRecord {
                id: RenderHistoryId::try_new(1).unwrap(),
                project_id: None,
                recipe,
                repo_name: utils::project_name("project"),
                label_parts: RecipeLabelParts::None,
                range_label: "main..HEAD".into(),
                rendered_at: "2026-09-20T00:00:00Z".parse().unwrap(),
            };
            state
                .update(|session| {
                    session.bind_snapshot_history(work.ticket(), &record);
                    let tab = session.tab(work.ticket().tab_id).unwrap();
                    assert_eq!(tab.tab.label(), &named(expected));
                    assert_eq!(tab.recipe.name.as_ref().unwrap().as_str(), expected);
                })
                .unwrap();
        }
    }

    #[test]
    fn rename_persists_history_without_changing_tab_identity() {
        let (state, connection, tab_id) = fixture(ViewerTabKind::Snapshot);
        rename_snapshot::execute(
            &RenameViewerSnapshot {
                tab_id,
                name: "  Review auth  ".into(),
            },
            &state,
            &connection,
        )
        .unwrap();
        let stored: String = connection
            .query_row(
                "SELECT recipe_name FROM recent_renders WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, "Review auth");
        state
            .inspect(|session| {
                let tab = session.tab(tab_id).unwrap();
                assert_eq!(tab.tab.label(), &named("Review auth"));
                assert_eq!(tab.recipe.name.as_ref().unwrap().as_str(), "Review auth");
            })
            .unwrap();
    }

    #[test]
    fn a_missing_history_record_keeps_the_label() {
        let (state, connection, tab_id) = fixture(ViewerTabKind::Snapshot);
        connection
            .execute_batch("DELETE FROM recent_renders")
            .unwrap();
        assert!(matches!(
            rename_snapshot::execute(
                &RenameViewerSnapshot {
                    tab_id,
                    name: "Lost rename".into()
                },
                &state,
                &connection
            ),
            Err(RenameSnapshotError::HistoryMissing)
        ));
        state
            .inspect(|session| {
                assert_eq!(session.tab(tab_id).unwrap().tab.label(), &two_commits());
            })
            .unwrap();
    }

    #[test]
    fn live_tabs_and_invalid_names_cannot_be_renamed() {
        let (state, connection, tab_id) = fixture(ViewerTabKind::Live);
        assert!(matches!(
            rename_snapshot::execute(
                &RenameViewerSnapshot {
                    tab_id,
                    name: "Live name".into()
                },
                &state,
                &connection
            ),
            Err(RenameSnapshotError::NotSnapshot)
        ));
        for name in [" ".into(), "two\nlines".into(), "a".repeat(201)] {
            assert!(matches!(
                rename_snapshot::execute(
                    &RenameViewerSnapshot { tab_id, name },
                    &state,
                    &connection
                ),
                Err(RenameSnapshotError::InvalidName)
            ));
        }
    }
}
