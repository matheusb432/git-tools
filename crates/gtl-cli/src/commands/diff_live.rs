//! `gtl diff live`: validate and persist a live view of unpushed work through
//! `gtl-server`, then forward it to the Dioxus viewer shell.
//! `--path <p>` saves one repo; with no path, every managed repo with unpushed
//! commits is saved and forwarded as one batch.

use anyhow::Context as _;
use gtl_models::{
    live_views::LiveSource,
    paths::{ProjectName, RepositoryRoot},
    recipes::RecipeBatchId,
};
use gtl_wire::{
    recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget},
    v1,
};

use crate::server_client::ServerClient;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SavedLiveView {
    source: LiveSource,
    display_name: ProjectName,
}

/// Save + open a live view: `path` targets one repo, `None` fans out over every
/// managed repo with unpushed commits.
///
/// # Errors
/// Returns an error when the server cannot be reached, or (for `--path`) the save
/// is rejected. The rejection keeps the server's service-composed message,
/// printed verbatim by the caller's exit path.
pub fn run(path: Option<String>) -> anyhow::Result<()> {
    let client = ServerClient::connect()?;
    match path {
        Some(path) => run_single(&client, &path),
        None => run_managed(&client),
    }
}

/// `--path <p>`: save one live view and forward it alone. A rejected save
/// propagates as an error and forwards nothing.
fn run_single(client: &ServerClient, path: &str) -> anyhow::Result<()> {
    let data = save_one(client, path)?;
    let batch = OpenRecipes {
        batch_id: RecipeBatchId::generate(),
        kind: RecipeBatchKind::Live,
        recipes: vec![live_recipe(&data)],
    };
    forward_or_degrade(&batch);
    Ok(())
}

/// No `--path`: save every managed repo with unpushed commits, forwarding the ones that
/// saved successfully as one batch.
fn run_managed(client: &ServerClient) -> anyhow::Result<()> {
    let response = client.save_project_live_views()?;
    if response.results.is_empty() {
        println!("diff live: no managed repos with unpushed commits");
        return Ok(());
    }

    let mut recipes = Vec::new();
    for result in response.results {
        match saved_from_response(result) {
            Ok(data) => recipes.push(live_recipe(&data)),
            Err(err) => eprintln!("diff live: {err:#}"),
        }
    }

    if recipes.is_empty() {
        anyhow::bail!("diff live: no live view could be saved");
    }

    let batch = OpenRecipes {
        batch_id: RecipeBatchId::generate(),
        kind: RecipeBatchKind::Live,
        recipes,
    };
    forward_or_degrade(&batch);
    Ok(())
}

/// Validate and persist one live-view source. On success, prints
/// the response notes and returns the saved data. A rejection becomes the
/// service-composed error text and is printed once by the caller's exit path.
fn save_one(client: &ServerClient, path: &str) -> anyhow::Result<SavedLiveView> {
    let request = v1::SaveLiveViewRequest {
        path: std::path::absolute(path)?.to_string_lossy().into_owned(),
    };
    saved_from_response(client.save_live_view(request)?)
}

fn saved_from_response(response: v1::SaveLiveViewResponse) -> anyhow::Result<SavedLiveView> {
    match response
        .outcome
        .context("gtl-server returned no live-view outcome")?
    {
        v1::save_live_view_response::Outcome::Saved(saved) => {
            print_notes(&response.notes)?;
            match v1::SaveLiveViewDisposition::try_from(saved.disposition) {
                Ok(
                    v1::SaveLiveViewDisposition::Created | v1::SaveLiveViewDisposition::Refreshed,
                ) => {}
                Ok(v1::SaveLiveViewDisposition::Unspecified) | Err(_) => {
                    anyhow::bail!("gtl-server returned an invalid live-view disposition")
                }
            }
            let repository_root = RepositoryRoot::try_new(saved.repository_root.into())
                .context("gtl-server returned a non-absolute live-view repository root")?;
            let display_name = ProjectName::try_new(saved.display_name)
                .context("gtl-server returned an empty live-view display name")?;
            Ok(SavedLiveView {
                source: LiveSource::local_repo(repository_root),
                display_name,
            })
        }
        v1::save_live_view_response::Outcome::Rejected(rejection) => {
            anyhow::bail!(rejection.detail)
        }
    }
}

fn print_notes(notes: &[v1::Note]) -> anyhow::Result<()> {
    for note in notes {
        match v1::NoteLevel::try_from(note.level) {
            Ok(v1::NoteLevel::Info) => println!("{}", note.text),
            Ok(v1::NoteLevel::Warning) => eprintln!("{}", note.text),
            Ok(v1::NoteLevel::Error) => anyhow::bail!(note.text.clone()),
            Ok(v1::NoteLevel::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid live-view note level")
            }
        }
    }
    Ok(())
}

/// Map a saved live view onto the recipe that renders its unpushed work.
fn live_recipe(data: &SavedLiveView) -> Recipe {
    let gtl_models::live_views::LiveSource::LocalRepo { path } = &data.source;
    Recipe {
        source: RecipeSource::LocalRepo(path.clone()),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: Some(data.display_name.clone()),
    }
}

/// Forward `batch`; a forward failure (no viewer installed, spawn failed) is a
/// low-noise degrade, not a command failure — every live view in the batch is
/// already durably saved by the time this runs. Unlike the other
/// `diff` family commands, `diff live` has no browser fallback (live views are
/// app-only, by design), so it prints its own note rather than
/// [`super::note_viewer_degrade`] — that shared note falsely claims a browser
/// render `diff live` never performs.
fn forward_or_degrade(batch: &OpenRecipes) {
    if let Err(err) = super::forward_recipes(batch) {
        eprintln!("{}", live_degrade_note(&err));
    }
}

/// The message printed when forwarding a saved live view to the viewer fails. Pure
/// (no I/O) so tests can assert on its exact text without capturing stderr.
fn live_degrade_note(err: &anyhow::Error) -> String {
    format!(
        "diff live: viewer unavailable ({err:#}); the live view is saved and will open when the viewer is available."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{project_name, repository_root};

    #[test]
    fn live_degrade_note_conveys_the_save_without_claiming_a_browser_render() {
        let note = live_degrade_note(&anyhow::anyhow!("gtl-viewer is not installed"));

        assert!(
            !note.contains("browser"),
            "diff live has no browser fallback; the note must not claim one: {note}"
        );
        assert!(
            note.contains("saved") && note.contains("open"),
            "the note must convey the view was saved and will open later: {note}"
        );
    }

    #[test]
    fn live_recipe_maps_the_typed_source_to_an_unpushed_diff_recipe() {
        let data = SavedLiveView {
            source: LiveSource::local_repo(repository_root("/repos/three")),
            display_name: project_name("three"),
        };

        assert_eq!(
            live_recipe(&data),
            Recipe {
                source: RecipeSource::LocalRepo(repository_root("/repos/three")),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None }
                },
                name: Some(project_name("three")),
            }
        );
    }
}
