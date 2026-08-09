//! `gtl diff live`: validate + persist a live view of unpushed work through the
//! daemon's `POST /live-views/save`, then forward it to the Dioxus viewer shell.
//! `--path <p>` saves one repo; with no path, every managed repo with unpushed
//! commits is saved and forwarded as one batch.

use anyhow::Context as _;
use gtl_contracts::{
    envelope::Outcome,
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
    recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget},
};

use crate::client::HttpClient;

/// Save + open a live view: `path` targets one repo, `None` fans out over every
/// managed repo with unpushed commits.
///
/// # Errors
/// Returns an error when the daemon can't be reached, or (for `--path`) the save
/// is rejected — the error text is the daemon's own service-composed message,
/// printed verbatim by the caller's exit path.
pub fn run(path: Option<String>) -> anyhow::Result<()> {
    let client = HttpClient::ensure_daemon()?;
    match path {
        Some(path) => run_single(&client, &path),
        None => run_managed(&client),
    }
}

/// `--path <p>`: save one live view and forward it alone. A rejected save
/// propagates as an error and forwards nothing.
fn run_single(client: &HttpClient, path: &str) -> anyhow::Result<()> {
    let data = save_one(client, path)?;
    let batch = OpenRecipes {
        batch_id: crate::recipe::new_batch_id(),
        kind: RecipeBatchKind::Live,
        recipes: vec![live_recipe(&data)],
    };
    forward_or_degrade(&batch);
    Ok(())
}

/// No `--path`: save every managed repo with unpushed commits, forwarding the ones that
/// saved successfully as one batch.
fn run_managed(client: &HttpClient) -> anyhow::Result<()> {
    let tops = crate::recipe::selected_managed_repos()?;
    if tops.is_empty() {
        println!("diff live: no managed repos with unpushed commits");
        return Ok(());
    }

    let mut recipes = Vec::new();
    for repo_top in tops {
        let path = repo_top.path.to_string_lossy().into_owned();
        match save_one(client, &path) {
            Ok(data) => recipes.push(live_recipe(&data)),
            // One repo's rejection doesn't fail the whole batch; `save_one` withholds
            // the rejection notes on error, so print the composed message once here.
            Err(err) => eprintln!("diff live: {err:#}"),
        }
    }

    if recipes.is_empty() {
        anyhow::bail!("diff live: no live view could be saved");
    }

    let batch = OpenRecipes {
        batch_id: crate::recipe::new_batch_id(),
        kind: RecipeBatchKind::Live,
        recipes,
    };
    forward_or_degrade(&batch);
    Ok(())
}

/// Validate and persist one live-view source. On success, prints
/// the envelope's wire notes and returns the saved data. A rejection
/// (`Outcome::Error`) becomes the service-composed error text, printed once by the
/// caller's exit path, so the notes are deliberately NOT printed here; any other
/// non-`Ok` outcome is treated the same way (the endpoint never returns `Empty`).
fn save_one(client: &HttpClient, path: &str) -> anyhow::Result<SaveLiveViewData> {
    let request = SaveLiveViewRequest {
        path: std::path::absolute(path)?.to_string_lossy().into_owned(),
    };
    let envelope = client.save_live_view(&request)?;
    match envelope.outcome {
        // Print the success (Info) notes and surface the data. On rejection we do NOT
        // print the notes: the caller turns `error_text` (which now falls back to the
        // rejection's Warn note) into the returned error, printed once by the exit path;
        // printing here too would double up the message.
        Outcome::Ok => {
            super::print_wire_notes(&envelope.notes);
            envelope.data.context("daemon returned ok without data")
        }
        _ => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

/// Map a saved live view onto the recipe that renders its unpushed work.
fn live_recipe(data: &SaveLiveViewData) -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(data.source_value.clone().into()),
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
    fn live_recipe_maps_source_value_to_an_unpushed_diff_recipe() {
        let data = SaveLiveViewData {
            source_kind: "LocalRepo".to_string(),
            source_value: "/repos/three".to_string(),
            display_name: "three".to_string(),
            already_saved: true,
        };

        assert_eq!(
            live_recipe(&data),
            Recipe {
                source: RecipeSource::LocalRepo("/repos/three".into()),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None }
                },
                name: Some("three".into()),
            }
        );
    }
}
