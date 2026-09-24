use gtl_models::{
    failure::{ErrorMeta, Failure, Resource, ViewerFailure},
    paths::ProjectName,
};
use gtl_wire::viewer::RenameViewerSnapshot;
use rusqlite::{Connection, params};

use super::{ViewerState, ViewerStateError, ViewerTabKind, pinned_tabs};

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
    connection: &mut Connection,
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
        let records = session.tabs().filter(|tab| tab.pinned).map(|tab| {
            let mut recipe = tab.recipe.clone();
            if tab.history_id == Some(history_id) {
                recipe.name = Some(name.clone());
            }
            (recipe, tab.tab.kind() == ViewerTabKind::Live)
        }).collect::<Vec<_>>();
        let transaction = connection.transaction().map_err(anyhow::Error::from)?;
        let changed = transaction.execute(
            "UPDATE recent_renders SET recipe_name = ?1, title = ?1 WHERE id = ?2 AND render_status = 'success'",
            params![name.as_str(), i64::from(history_id)],
        ).map_err(anyhow::Error::from)?;
        if changed != 1 {
            return Err(RenameSnapshotError::HistoryMissing);
        }
        pinned_tabs::persist_records(&transaction, &records).map_err(anyhow::Error::from)?;
        transaction.commit().map_err(anyhow::Error::from)?;
        session.rename_snapshot(history_id, &name);
        Ok(())
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::{recipes::RecipeBatchId, viewer::RenderHistoryId};

    use super::*;
    use crate::{
        history::RecentRenderRecord,
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{rename_snapshot, session::CachedView, work},
    };

    fn fixture(kind: ViewerTabKind) -> (ViewerState, Connection, gtl_models::viewer::ViewerTabId) {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE recent_renders (id INTEGER PRIMARY KEY, recipe_name TEXT, title TEXT, render_status TEXT);
            INSERT INTO recent_renders VALUES (1, NULL, 'project: 2 commits', 'success');
            CREATE TABLE pinned_viewer_tabs (position INTEGER PRIMARY KEY, recipe_json TEXT, live INTEGER);").unwrap();
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
                    "project: 2 commits".into(),
                );
                session.bind_snapshot_history(
                    ticket,
                    &RecentRenderRecord {
                        id: RenderHistoryId::try_new(1).unwrap(),
                        project_id: None,
                        recipe,
                        title: "project: 2 commits".into(),
                        repo_name: utils::project_name("project"),
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
                title: "Saved review".into(),
                repo_name: utils::project_name("project"),
                range_label: "main..HEAD".into(),
                rendered_at: "2026-09-20T00:00:00Z".parse().unwrap(),
            };
            state
                .update(|session| {
                    session.bind_snapshot_history(work.ticket(), &record);
                    let tab = session.tab(work.ticket().tab_id).unwrap();
                    assert_eq!(tab.tab.label(), expected);
                    assert_eq!(tab.recipe.name.as_ref().unwrap().as_str(), expected);
                })
                .unwrap();
        }
    }

    #[test]
    fn rename_persists_history_and_pins_without_changing_tab_identity() {
        let (state, mut connection, tab_id) = fixture(ViewerTabKind::Snapshot);
        pinned_tabs::execute(
            gtl_wire::viewer::SetViewerTabPinned {
                tab_id,
                pinned: true,
            },
            &state,
            &mut connection,
        )
        .unwrap();
        rename_snapshot::execute(
            &RenameViewerSnapshot {
                tab_id,
                name: "  Review auth  ".into(),
            },
            &state,
            &mut connection,
        )
        .unwrap();
        let stored: (String, String) = connection
            .query_row(
                "SELECT recipe_name, title FROM recent_renders WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, ("Review auth".into(), "Review auth".into()));
        let restored = ViewerState::new();
        let restored_work = pinned_tabs::restore(&restored, &connection).unwrap();
        assert_eq!(
            restored_work[0].recipe().name.as_ref().unwrap().as_str(),
            "Review auth"
        );
        state
            .inspect(|session| {
                let tab = session.tab(tab_id).unwrap();
                assert_eq!(tab.tab.label(), "Review auth");
                assert_eq!(tab.recipe.name.as_ref().unwrap().as_str(), "Review auth");
                assert!(tab.pinned);
            })
            .unwrap();
    }

    #[test]
    fn failed_pin_write_rolls_back_history_and_keeps_the_label() {
        let (state, mut connection, tab_id) = fixture(ViewerTabKind::Snapshot);
        connection
            .execute_batch("DROP TABLE pinned_viewer_tabs")
            .unwrap();
        assert!(
            rename_snapshot::execute(
                &RenameViewerSnapshot {
                    tab_id,
                    name: "Lost rename".into()
                },
                &state,
                &mut connection
            )
            .is_err()
        );
        let title: String = connection
            .query_row("SELECT title FROM recent_renders WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "project: 2 commits");
        state
            .inspect(|session| assert_eq!(session.tab(tab_id).unwrap().tab.label(), title))
            .unwrap();
    }

    #[test]
    fn live_tabs_and_invalid_names_cannot_be_renamed() {
        let (state, mut connection, tab_id) = fixture(ViewerTabKind::Live);
        assert!(matches!(
            rename_snapshot::execute(
                &RenameViewerSnapshot {
                    tab_id,
                    name: "Live name".into()
                },
                &state,
                &mut connection
            ),
            Err(RenameSnapshotError::NotSnapshot)
        ));
        for name in [" ".into(), "two\nlines".into(), "a".repeat(201)] {
            assert!(matches!(
                rename_snapshot::execute(
                    &RenameViewerSnapshot { tab_id, name },
                    &state,
                    &mut connection
                ),
                Err(RenameSnapshotError::InvalidName)
            ));
        }
    }
}
