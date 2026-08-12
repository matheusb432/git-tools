use gtl_application::{live_views, viewer::ViewerTabKind};
use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::presentation::ViewerApp;

pub(crate) fn restore(app: &ViewerApp) -> Result<bool, String> {
    app.restoration.run_once(|| {
        let records = {
            let connection = app
                .app_state
                .connection_lock()
                .map_err(|error| format!("{error:#}"))?;
            live_views::list::execute(live_views::list::ListLiveViews, &connection)
                .map_err(|error| format!("{error:#}"))?
                .views
        };
        let mut newest = None;
        {
            let mut session = app.session.lock().map_err(|error| error.to_string())?;
            for record in records {
                let Some(recipe) = restored_recipe(record) else {
                    continue;
                };
                newest = Some(
                    session
                        .open(recipe, "restored-live".into(), ViewerTabKind::Live)
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

fn restored_recipe(record: live_views::LiveViewRecord) -> Option<Recipe> {
    if record.source_kind != "LocalRepo" {
        return None;
    }
    Some(Recipe {
        source: RecipeSource::LocalRepo(record.source_value.into()),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: Some(record.display_name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(source_kind: &str) -> live_views::LiveViewRecord {
        live_views::LiveViewRecord {
            source_kind: source_kind.into(),
            source_value: "/repos/gt".into(),
            display_name: "git-tools".into(),
            created_at: "2026-08-09T12:00:00Z".into(),
            last_opened_at: None,
        }
    }

    #[test]
    fn persisted_local_repository_restores_as_an_unpushed_live_recipe() {
        let recipe = restored_recipe(record("LocalRepo")).expect("supported live source");

        assert_eq!(recipe.name.as_deref(), Some("git-tools"));
        assert!(matches!(
            recipe,
            Recipe {
                source: RecipeSource::LocalRepo(path),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None }
                },
                ..
            } if path == std::path::Path::new("/repos/gt")
        ));
    }

    #[test]
    fn unknown_persisted_source_kind_is_ignored() {
        assert!(restored_recipe(record("RemoteRepo")).is_none());
    }
}
