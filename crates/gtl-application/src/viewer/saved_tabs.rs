//! Saves every open viewer tab so a restarted server restores its tab strip.

use gtl_models::{
    failure::ErrorMeta,
    recipes::{RecipeBatchId, RecipeLabel},
};
use rusqlite::{Connection, params};

use super::{
    ViewerState,
    session::ViewerSession,
    work::{self, ReserveRecipeError, ReservedRecipeWork},
};
use crate::recipes::{Recipe, recipe_label};

/// One open tab with the order, flags, and label the viewer restores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedViewerTab {
    pub recipe: Recipe,
    pub label: RecipeLabel,
    pub pinned: bool,
    pub live: bool,
    pub active: bool,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SaveViewerTabsError {
    #[error(transparent)]
    #[meta(private(Internal))]
    Storage(#[from] rusqlite::Error),
    #[error(transparent)]
    #[meta(private(Internal))]
    Encode(#[from] serde_json::Error),
    #[error("viewer tab position {position} exceeds the storage range")]
    #[meta(private(Internal))]
    PositionOverflow { position: usize },
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum LoadViewerTabsError {
    #[error(transparent)]
    #[meta(private(Internal))]
    Storage(#[from] rusqlite::Error),
    #[error("saved viewer tab at position {position} has an invalid {field}: {reason}")]
    #[meta(private(DataLoss))]
    InvalidRow {
        position: i64,
        field: &'static str,
        reason: String,
    },
}

/// Lists the session's tabs in strip order.
#[must_use]
pub fn project(session: &ViewerSession) -> Vec<SavedViewerTab> {
    session
        .tabs()
        .map(|tab| SavedViewerTab {
            recipe: tab.recipe.clone(),
            label: tab.tab.label().clone(),
            pinned: tab.pinned,
            live: tab.tab.live(),
            active: session.active() == Some(tab.tab.id()),
        })
        .collect()
}

/// Replaces the saved tab strip with `tabs`.
pub fn save(
    connection: &mut Connection,
    tabs: &[SavedViewerTab],
) -> Result<(), SaveViewerTabsError> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM viewer_tabs", [])?;
    {
        let mut statement = transaction.prepare_cached(
            "INSERT INTO viewer_tabs (position, recipe_json, label_json, pinned, live, active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (position, tab) in tabs.iter().enumerate() {
            statement.execute(params![
                i64::try_from(position)
                    .map_err(|_| SaveViewerTabsError::PositionOverflow { position })?,
                serde_json::to_string(&tab.recipe)?,
                serde_json::to_string(&tab.label)?,
                tab.pinned,
                tab.live,
                tab.active,
            ])?;
        }
    }
    transaction.commit()?;
    Ok(())
}

/// Reads the saved tab strip in order.
pub fn load(connection: &Connection) -> Result<Vec<SavedViewerTab>, LoadViewerTabsError> {
    let mut statement = connection.prepare(
        "SELECT position, recipe_json, label_json, pinned, live, active
         FROM viewer_tabs ORDER BY position",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, bool>(5)?,
        ))
    })?;
    rows.map(|row| {
        let (position, recipe, label, pinned, live, active) = row?;
        let invalid = |field, error: serde_json::Error| LoadViewerTabsError::InvalidRow {
            position,
            field,
            reason: error.to_string(),
        };
        let recipe: Recipe =
            serde_json::from_str(&recipe).map_err(|error| invalid("recipe", error))?;
        let label = match label {
            Some(label) => serde_json::from_str(&label).map_err(|error| invalid("label", error))?,
            None => recipe_label::pending_tab(&recipe),
        };
        Ok(SavedViewerTab {
            recipe,
            label,
            pinned,
            live,
            active,
        })
    })
    .collect()
}

/// Reopens `tabs` in order and reserves work for every restored tab, the active tab first.
pub fn restore(
    state: &ViewerState,
    tabs: Vec<SavedViewerTab>,
) -> Result<Vec<ReservedRecipeWork>, ReserveRecipeError> {
    state.update(|session| {
        let mut ids = Vec::with_capacity(tabs.len());
        let mut active = None;
        for tab in tabs {
            let id = session
                .open_labeled(tab.recipe, RecipeBatchId::generate(), tab.label)
                .ok_or(ReserveRecipeError::TabIdentifiersExhausted)?;
            session.set_live(id, tab.live);
            if tab.pinned {
                session.set_pinned(id, true);
            }
            if tab.active {
                active = Some(id);
            }
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if let Some(id) = active {
            session.activate(id);
            ids.retain(|restored| *restored != id);
            ids.insert(0, id);
        }
        ids.into_iter()
            .map(|id| work::reserve_refresh_in_session(session, id))
            .collect()
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerTabState;

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils,
    };

    fn store() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE viewer_tabs (
                    position INTEGER PRIMARY KEY,
                    recipe_json TEXT NOT NULL,
                    label_json TEXT,
                    pinned INTEGER NOT NULL,
                    live INTEGER NOT NULL,
                    active INTEGER NOT NULL
                ) STRICT",
            )
            .unwrap();
        connection
    }

    fn saved(path: &str, pinned: bool, live: bool, active: bool) -> SavedViewerTab {
        let recipe = crate::recipes::Recipe {
            source: crate::recipes::RecipeSource::LocalRepo(utils::repository_root(path)),
            ..utils::viewer::recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            })
        };
        SavedViewerTab {
            label: utils::viewer::label(path),
            recipe,
            pinned,
            live,
            active,
        }
    }

    #[test]
    fn saved_tabs_round_trip_in_strip_order() {
        let mut connection = store();
        let tabs = vec![
            saved("/repos/pinned", true, true, false),
            saved("/repos/active", false, false, true),
            saved("/repos/other", false, true, false),
        ];

        save(&mut connection, &tabs).unwrap();
        save(&mut connection, &tabs).unwrap();

        assert_eq!(load(&connection).unwrap(), tabs);
    }

    #[test]
    fn a_tab_without_a_saved_label_shows_its_pending_label() {
        let connection = store();
        let tab = saved("/repos/project", false, false, false);
        connection
            .execute(
                "INSERT INTO viewer_tabs VALUES (0, ?1, NULL, 0, 0, 0)",
                [serde_json::to_string(&tab.recipe).unwrap()],
            )
            .unwrap();

        assert_eq!(
            load(&connection).unwrap()[0].label,
            recipe_label::pending_tab(&tab.recipe)
        );
    }

    #[test]
    fn an_undecodable_recipe_is_reported_with_its_position() {
        let connection = store();
        connection
            .execute_batch(r#"INSERT INTO viewer_tabs VALUES (3, '{"source":{}}', NULL, 0, 0, 0)"#)
            .unwrap();

        assert!(matches!(
            load(&connection),
            Err(LoadViewerTabsError::InvalidRow {
                position: 3,
                field: "recipe",
                ..
            })
        ));
    }

    #[test]
    fn restore_reopens_every_tab_and_computes_the_active_one_first() {
        let state = ViewerState::new();
        let tabs = vec![
            saved("/repos/pinned", true, false, false),
            saved("/repos/active", false, false, true),
            saved("/repos/live", false, true, false),
        ];

        let work = restore(&state, tabs.clone()).unwrap();

        state
            .inspect(|session| {
                assert_eq!(project(session), tabs);
                let order = work
                    .iter()
                    .map(|work| session.tab(work.ticket().tab_id).unwrap().recipe.cwd())
                    .collect::<Vec<_>>();
                assert_eq!(
                    order,
                    ["/repos/active", "/repos/pinned", "/repos/live"].map(utils::repository_root)
                );
                assert_eq!(session.active(), Some(work[0].ticket().tab_id));
                assert!(
                    session
                        .tabs()
                        .all(|tab| tab.tab.state() == &ViewerTabState::Pending)
                );
                assert_eq!(session.focus_request_version(), None);
            })
            .unwrap();
    }
}
