//! `gtl diff live`: validate + persist a live view of unpushed work through the
//! daemon's `POST /live-views/save`, then forward it to the viewer app for HTMX
//! rendering. `--path <p>` saves one repo; with no path, every managed repo with
//! unpushed commits is saved and forwarded as one batch.

use anyhow::Context as _;
use contracts::{
    envelope::Outcome,
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
};
use gtl_recipe::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget};

use crate::{
    client::{Backend, HttpBackend},
    commands::managed::ManagedOptions,
};

/// Save + open a live view: `path` targets one repo, `None` fans out over every
/// managed repo with unpushed commits.
///
/// # Errors
/// Returns an error when the daemon can't be reached, or (for `--path`) the save
/// is rejected — the error text is the daemon's own service-composed message,
/// printed verbatim by the caller's exit path.
pub fn run(path: Option<String>) -> anyhow::Result<()> {
    let backend = HttpBackend::ensure_daemon()?;
    run_with(&backend, path)
}

/// [`run`], split so tests can drive a fake [`Backend`]. Forwards through the
/// real viewer app.
pub(crate) fn run_with(backend: &impl Backend, path: Option<String>) -> anyhow::Result<()> {
    run_with_forward(backend, path, super::forward_recipes)
}

/// The save/forward logic with the viewer forward injected, so tests can assert
/// on the forwarded batch instead of spawning a real `gtl-viewer`.
fn run_with_forward(
    backend: &impl Backend,
    path: Option<String>,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    match path {
        Some(path) => run_single(backend, &path, forward),
        None => run_managed(backend, forward),
    }
}

/// `--path <p>`: save one live view and forward it alone. A rejected save
/// propagates as an error and forwards nothing.
fn run_single(
    backend: &impl Backend,
    path: &str,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let data = save_one(backend, path)?;
    let batch = OpenRecipes {
        batch_id: crate::recipe::new_batch_id(),
        kind: RecipeBatchKind::Live,
        recipes: vec![live_recipe(&data)],
    };
    forward_or_degrade(&batch, forward);
    Ok(())
}

/// No `--path`: save every managed repo with unpushed commits (default manifest
/// lookup), forwarding the ones that saved successfully as one batch.
fn run_managed(
    backend: &impl Backend,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let options = ManagedOptions {
        repos_file: None,
        home_dir: None,
        dry: false,
        json: false,
        color: false,
        message_for_all: None,
        interactive: false,
    };
    run_managed_with_options(backend, &options, forward)
}

/// [`run_managed`], with [`ManagedOptions`] injected so tests can point the
/// manifest lookup at a fixture instead of the real `$HOME`.
fn run_managed_with_options(
    backend: &impl Backend,
    options: &ManagedOptions,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let tops = crate::commands::diff_subrepos::unpushed_managed_repo_tops(options)?;
    if tops.is_empty() {
        println!("diff live: no managed repos with unpushed commits");
        return Ok(());
    }

    let mut recipes = Vec::new();
    for repo_top in tops {
        let path = repo_top.top.to_string_lossy().into_owned();
        match save_one(backend, &path) {
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
    forward_or_degrade(&batch, forward);
    Ok(())
}

/// Validate + persist one live-view source through `backend`. On success, prints
/// the envelope's wire notes and returns the saved data. A rejection
/// (`Outcome::Error`) becomes the service-composed error text — printed once by the
/// caller's exit path, so the notes are deliberately NOT printed here; any other
/// non-`Ok` outcome is treated the same way (the endpoint never returns `Empty`).
fn save_one(backend: &impl Backend, path: &str) -> anyhow::Result<SaveLiveViewData> {
    let req = SaveLiveViewRequest {
        data_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        path: std::path::absolute(path)?.to_string_lossy().into_owned(),
    };
    let envelope = backend.save_live_view(&req)?;
    match envelope.outcome {
        // Print the success (Info) notes and surface the data. On rejection we do NOT
        // print the notes: the caller turns `error_text` (which now falls back to the
        // rejection's Warn note) into the returned error, printed once by the exit path —
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
            target: RecipeTarget::Unpushed,
        },
        name: Some(data.display_name.clone()),
    }
}

/// Forward `batch`; a forward failure (no viewer installed, spawn failed) is a
/// low-noise degrade, not a command failure — every live view in the batch is
/// already durably saved by the time this runs (FSD A-0002). Unlike the other
/// `diff` family commands, `diff live` has no browser fallback (live views are
/// app-only, by design), so it prints its own note rather than
/// [`super::note_viewer_degrade`] — that shared note falsely claims a browser
/// render `diff live` never performs.
fn forward_or_degrade(
    batch: &OpenRecipes,
    forward: impl FnOnce(&OpenRecipes) -> anyhow::Result<()>,
) {
    if let Err(err) = forward(batch) {
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
    use std::{
        cell::{Cell, RefCell},
        path::Path,
    };

    use contracts::envelope::{Envelope, Note, NoteLevel};

    use super::*;

    struct FakeBackend(Envelope<SaveLiveViewData>);

    impl Backend for FakeBackend {
        fn save_live_view(
            &self,
            _req: &SaveLiveViewRequest,
        ) -> anyhow::Result<Envelope<SaveLiveViewData>> {
            Ok(self.0.clone())
        }
    }

    /// Echoes the request's `path` back as `source_value`, so a managed fan-out
    /// test can assert the forwarded recipes match the repos that were saved.
    struct EchoingBackend;

    impl Backend for EchoingBackend {
        fn save_live_view(
            &self,
            req: &SaveLiveViewRequest,
        ) -> anyhow::Result<Envelope<SaveLiveViewData>> {
            Ok(Envelope {
                outcome: Outcome::Ok,
                notes: vec![Note {
                    level: NoteLevel::Info,
                    text: format!("saved live view for {}", req.path),
                }],
                data: Some(SaveLiveViewData {
                    source_kind: "LocalRepo".to_string(),
                    source_value: req.path.clone(),
                    display_name: "repo".to_string(),
                    already_saved: false,
                }),
            })
        }
    }

    struct RejectingBackend {
        reject_first: bool,
        calls: Cell<usize>,
    }

    impl Backend for RejectingBackend {
        fn save_live_view(
            &self,
            req: &SaveLiveViewRequest,
        ) -> anyhow::Result<Envelope<SaveLiveViewData>> {
            let call = self.calls.get();
            self.calls.set(call + 1);
            if !self.reject_first || call == 0 {
                return Ok(rejected_envelope("rejected repo"));
            }
            Ok(Envelope {
                outcome: Outcome::Ok,
                notes: vec![],
                data: Some(SaveLiveViewData {
                    source_kind: "LocalRepo".into(),
                    source_value: req.path.clone(),
                    display_name: "saved repo".into(),
                    already_saved: false,
                }),
            })
        }
    }

    fn ok_envelope(source_value: &str) -> Envelope<SaveLiveViewData> {
        Envelope {
            outcome: Outcome::Ok,
            notes: vec![Note {
                level: NoteLevel::Info,
                text: "live view saved".to_string(),
            }],
            data: Some(SaveLiveViewData {
                source_kind: "LocalRepo".to_string(),
                source_value: source_value.to_string(),
                display_name: "repo".to_string(),
                already_saved: false,
            }),
        }
    }

    /// A rejection as the daemon actually shapes it: `Outcome::Error` carrying the
    /// human message as a `Warn` note (the application layer has no `Error` level).
    fn rejected_envelope(text: &str) -> Envelope<SaveLiveViewData> {
        Envelope {
            outcome: Outcome::Error,
            notes: vec![Note {
                level: NoteLevel::Warn,
                text: text.to_string(),
            }],
            data: None,
        }
    }

    #[test]
    fn run_with_forward_saves_and_forwards_one_recipe_for_an_explicit_path() {
        let backend = FakeBackend(ok_envelope("/repos/one"));
        let captured: RefCell<Option<OpenRecipes>> = RefCell::new(None);

        run_with_forward(&backend, Some("/repos/one".to_string()), |batch| {
            *captured.borrow_mut() = Some(batch.clone());
            Ok(())
        })
        .unwrap();

        let batch = captured.into_inner().expect("forward must be called");
        assert_eq!(batch.kind, RecipeBatchKind::Live);
        assert_eq!(batch.recipes.len(), 1);
        assert_eq!(
            batch.recipes[0],
            Recipe {
                source: RecipeSource::LocalRepo("/repos/one".into()),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed
                },
                name: Some("repo".into()),
            }
        );
    }

    #[test]
    fn run_with_forward_returns_the_rejection_text_and_forwards_nothing() {
        let backend = FakeBackend(rejected_envelope("not a git repo"));

        let Err(err) = run_with_forward(&backend, Some("/nope".to_string()), |_batch| {
            unreachable!("a rejected save must not forward anything")
        }) else {
            panic!("a rejected save must map to Err")
        };

        assert_eq!(format!("{err:#}"), "not a git repo");
    }

    #[test]
    fn run_with_forward_degrades_when_forwarding_fails_after_a_successful_save() {
        let backend = FakeBackend(ok_envelope("/repos/one"));

        // The forward fails, but the save already happened — this must still be Ok.
        let result = run_with_forward(&backend, Some("/repos/one".to_string()), |_batch| {
            anyhow::bail!("gtl-viewer is not installed")
        });

        assert!(
            result.is_ok(),
            "a forward failure must not undo a successful save"
        );
    }

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

    fn init_repo(dir: &Path) {
        let g = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "first"]);
    }

    fn init_unpushed_managed_repo(home: &Path, name: &str, remote_root: &Path) {
        let repo = home.join(name);
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        let remote = remote_root.join(format!("{name}.git"));
        assert!(
            std::process::Command::new("git")
                .args(["init", "--bare", "-q"])
                .arg(&remote)
                .status()
                .unwrap()
                .success()
        );
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&["push", "-q", "-u", "origin", "HEAD"]);
        std::fs::write(repo.join("a.txt"), "a\nmore\n").unwrap();
        git(&["commit", "-aqm", "second"]);
    }

    fn two_repo_options(tmp: &Path) -> ManagedOptions {
        let home = tmp.join("home");
        init_unpushed_managed_repo(&home, "one", tmp);
        init_unpushed_managed_repo(&home, "two", tmp);
        let manifest = tmp.join("repos.toml");
        std::fs::write(
            &manifest,
            "[[repo]]\npath = \"one\"\nremote = \"origin\"\n[[repo]]\npath = \"two\"\nremote = \"origin\"\n",
        )
        .unwrap();
        ManagedOptions {
            repos_file: Some(manifest),
            home_dir: Some(home),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        }
    }

    #[test]
    fn managed_live_continues_after_a_rejection_and_forwards_saved_recipes() {
        let tmp = tempfile::tempdir().unwrap();
        let options = two_repo_options(tmp.path());
        let backend = RejectingBackend {
            reject_first: true,
            calls: Cell::new(0),
        };
        let captured = RefCell::new(None);

        run_managed_with_options(&backend, &options, |batch| {
            *captured.borrow_mut() = Some(batch.clone());
            Ok(())
        })
        .unwrap();

        assert_eq!(backend.calls.get(), 2);
        assert_eq!(captured.into_inner().unwrap().recipes.len(), 1);
    }

    #[test]
    fn managed_live_errors_when_every_candidate_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let options = two_repo_options(tmp.path());
        let backend = RejectingBackend {
            reject_first: false,
            calls: Cell::new(0),
        };

        let error = run_managed_with_options(&backend, &options, |_batch| {
            unreachable!("no batch exists when every save is rejected")
        })
        .unwrap_err();

        assert_eq!(backend.calls.get(), 2);
        assert_eq!(
            format!("{error:#}"),
            "diff live: no live view could be saved"
        );
    }

    #[test]
    fn run_managed_with_options_saves_and_forwards_one_recipe_per_unpushed_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join("repo1")).unwrap();
        init_repo(&home.join("repo1"));
        let remote = tmp.path().join("origin.git");
        assert!(
            std::process::Command::new("git")
                .args(["init", "--bare"])
                .arg(&remote)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(home.join("repo1"))
                .args(["remote", "add", "origin", remote.to_str().unwrap()])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(home.join("repo1"))
                .args(["push", "-u", "origin", "HEAD"])
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(home.join("repo1").join("a.txt"), "a\nmore\n").unwrap();
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(home.join("repo1"))
                .args(["commit", "-aqm", "second"])
                .status()
                .unwrap()
                .success()
        );

        let manifest = tmp.path().join("repos.toml");
        std::fs::write(
            &manifest,
            "[[repo]]\npath = \"repo1\"\nremote = \"origin\"\n",
        )
        .unwrap();
        let options = ManagedOptions {
            repos_file: Some(manifest),
            home_dir: Some(home.clone()),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        };
        let canonical_top = std::fs::canonicalize(home.join("repo1")).unwrap();
        let captured: RefCell<Option<OpenRecipes>> = RefCell::new(None);

        run_managed_with_options(&EchoingBackend, &options, |batch| {
            *captured.borrow_mut() = Some(batch.clone());
            Ok(())
        })
        .unwrap();

        let batch = captured.into_inner().expect("forward must be called");
        assert_eq!(batch.recipes.len(), 1);
        assert_eq!(
            batch.recipes[0],
            Recipe {
                source: RecipeSource::LocalRepo(canonical_top),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed
                },
                name: Some("repo".into()),
            }
        );
    }

    #[test]
    fn run_managed_with_options_is_a_clean_noop_when_nothing_is_unpushed() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let manifest = tmp.path().join("repos.toml");
        std::fs::write(&manifest, "").unwrap();
        let options = ManagedOptions {
            repos_file: Some(manifest),
            home_dir: Some(home),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        };

        let result = run_managed_with_options(&EchoingBackend, &options, |_batch| {
            unreachable!("nothing to forward when no repo has unpushed commits")
        });

        assert!(result.is_ok());
    }

    #[test]
    fn save_one_prints_notes_and_surfaces_the_source_value_on_success() {
        let backend = FakeBackend(ok_envelope("/repos/two"));
        let data = save_one(&backend, "/repos/two").unwrap();
        assert_eq!(data.source_value, "/repos/two");
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
                    target: RecipeTarget::Unpushed
                },
                name: Some("three".into()),
            }
        );
    }
}
