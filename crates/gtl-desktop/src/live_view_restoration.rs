use gtl_application::{live_views, live_views::list_live_views, viewer::ViewerTabKind};
use gtl_models::{live_views::LiveSource, recipes::RecipeBatchId};
use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::presentation::ViewerApp;

pub(crate) fn restore(app: &ViewerApp) -> Result<bool, String> {
    app.restoration.run_once(|| {
        let records = {
            let connection = app
                .app_state
                .connection_lock()
                .map_err(|error| format!("{error:#}"))?;
            list_live_views::execute(list_live_views::ListLiveViews, &connection)
                .map_err(|error| format!("{error:#}"))?
                .views
        };
        let mut newest = None;
        {
            let mut session = app.session.lock().map_err(|error| error.to_string())?;
            for record in records {
                newest = Some(
                    session
                        .open(
                            restored_recipe(record),
                            RecipeBatchId::generate(),
                            ViewerTabKind::Live,
                        )
                        .ok_or_else(|| "viewer tab ids exhausted".to_owned())?,
                );
            }
        }
        if let Some(tab_id) = newest {
            app.refresh_recipe(tab_id)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })
}

fn restored_recipe(record: live_views::LiveViewRecord) -> Recipe {
    let LiveSource::LocalRepo { path } = record.source;
    Recipe {
        source: RecipeSource::LocalRepo(path),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: Some(record.display_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{project_name, repository_root};

    fn record() -> live_views::LiveViewRecord {
        live_views::LiveViewRecord {
            source: LiveSource::local_repo(repository_root("/repos/gt")),
            display_name: project_name("git-tools"),
            created_at: gtl_models::timestamps::MachineTimestamp::try_from("2026-08-09T12:00:00Z")
                .expect("fixture creation timestamp is valid"),
            last_opened_at: None,
        }
    }

    #[test]
    fn persisted_local_repository_restores_as_an_unpushed_live_recipe() {
        let recipe = restored_recipe(record());

        assert_eq!(
            recipe.name.as_ref().map(|name| name.as_str()),
            Some("git-tools")
        );
        assert!(matches!(
            recipe,
            Recipe {
                source: RecipeSource::LocalRepo(path),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None }
                },
                ..
            } if path == repository_root("/repos/gt")
        ));
    }
}
