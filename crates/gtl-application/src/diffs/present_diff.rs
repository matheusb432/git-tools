use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    recipes::RecipeBatchId,
};

use crate::{
    ports::{DiffRenderOutcome, DiffRenderRequest, DiffViewerClient, GitClient},
    recipes::{Recipe, RecipeBatch, RecipeBatchKind, RecipeOp},
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRecipeIntent {
    pub repo_root: RepositoryRoot,
    pub operation: RecipeOp,
    pub name: Option<ProjectName>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentDiff {
    pub render: DiffRenderRequest,
    pub batch_id: RecipeBatchId,
    pub recipes: Vec<DiffRecipeIntent>,
    pub mode: DiffPresentationMode,
}

/// Selects the permitted presentation route for one diff operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffPresentationMode {
    /// Render an artifact without attempting to open the graphical viewer.
    ArtifactOnly,
    /// Open the viewer and render an artifact if forwarding fails.
    ViewerWithArtifactFallback,
}

/// Records why an artifact renderer handled the presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactPresentationRoute {
    /// The caller requested or required direct artifact rendering.
    Direct,
    /// Viewer forwarding failed with the captured error.
    ViewerFallback { error: String },
}

/// Reports the one presentation route that completed.
#[derive(Debug, Clone, PartialEq)]
pub enum PresentDiffOk {
    /// The recipe batch was forwarded to the viewer.
    Viewer { notes: Vec<Note> },
    /// Direct rendering produced an artifact or a deliberate empty result.
    Artifact {
        outcome: DiffRenderOutcome,
        route: ArtifactPresentationRoute,
        notes: Vec<Note>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PresentDiffError {
    #[error(transparent)]
    Viewer(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    command: PresentDiff,
    viewer: &impl DiffViewerClient,
    git: &impl GitClient,
) -> Result<PresentDiffOk, PresentDiffError> {
    if command.recipes.is_empty() {
        return Ok(PresentDiffOk::Artifact {
            outcome: DiffRenderOutcome::Empty,
            route: ArtifactPresentationRoute::Direct,
            notes: Vec::new(),
        });
    }
    if command.mode == DiffPresentationMode::ArtifactOnly {
        return render_artifact(&command.render, ArtifactPresentationRoute::Direct, viewer);
    }

    let batch = build_batch(command.batch_id, command.recipes, git);
    match viewer.forward(&batch) {
        Ok(()) => Ok(PresentDiffOk::Viewer { notes: Vec::new() }),
        Err(error) => render_artifact(
            &command.render,
            ArtifactPresentationRoute::ViewerFallback {
                error: format!("{error:#}"),
            },
            viewer,
        ),
    }
}

fn render_artifact(
    request: &DiffRenderRequest,
    route: ArtifactPresentationRoute,
    viewer: &impl DiffViewerClient,
) -> Result<PresentDiffOk, PresentDiffError> {
    let response = viewer.render(request)?;
    let mut notes = response.notes;
    if let ArtifactPresentationRoute::ViewerFallback { error } = &route {
        notes.insert(
            0,
            Note::warn(format!(
                "diff: viewer unavailable ({error}); rendering an artifact instead"
            )),
        );
    }
    Ok(PresentDiffOk::Artifact {
        outcome: response.outcome,
        route,
        notes,
    })
}

fn build_batch(
    batch_id: RecipeBatchId,
    intents: Vec<DiffRecipeIntent>,
    git: &impl GitClient,
) -> RecipeBatch {
    let recipes = intents
        .into_iter()
        .map(|intent| build_recipe(intent, git))
        .collect();
    RecipeBatch {
        batch_id,
        kind: RecipeBatchKind::Snapshot,
        recipes,
    }
}

fn build_recipe(intent: DiffRecipeIntent, git: &impl GitClient) -> Recipe {
    crate::recipes::build_resolved(intent.repo_root, intent.operation, intent.name, git)
}

#[cfg(test)]
mod tests {
    use gtl_models::recipes::RecipeBatchId;

    use super::*;
    use crate::{
        diffs::{DiffTargetRequest, present_diff, render_diff::RenderDiff},
        ports::{DiffRenderResponse, DiffViewerClient},
        recipes::{RecipeBatch, RecipeTarget},
        utils::FakeGitClient,
    };

    #[derive(Debug, Clone)]
    struct FakeViewer {
        forward_error: Option<String>,
        render: DiffRenderOutcome,
    }

    impl DiffViewerClient for FakeViewer {
        fn forward(&self, _batch: &RecipeBatch) -> anyhow::Result<()> {
            match &self.forward_error {
                Some(error) => anyhow::bail!("{error}"),
                None => Ok(()),
            }
        }

        fn render(&self, _request: &DiffRenderRequest) -> anyhow::Result<DiffRenderResponse> {
            Ok(DiffRenderResponse {
                outcome: self.render.clone(),
                notes: vec![Note::info("rendered")],
            })
        }
    }

    fn request(mode: DiffPresentationMode) -> PresentDiff {
        PresentDiff {
            render: DiffRenderRequest::Diff(RenderDiff {
                cwd: "/repo".into(),
                target: DiffTargetRequest::Unpushed,
                name: None,
            }),
            batch_id: "0198a859-7c4e-7e5f-9e63-ec7bb768d841"
                .parse::<RecipeBatchId>()
                .expect("fixture batch ID is valid"),
            recipes: vec![DiffRecipeIntent {
                repo_root: crate::utils::repository_root("/repo"),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: None,
            }],
            mode,
        }
    }

    fn git() -> FakeGitClient {
        FakeGitClient {
            top_level: Some("/repo".into()),
            commit_ids: [
                ("@{u}".to_string(), crate::utils::commit_id_fixture("base")),
                ("HEAD".to_string(), crate::utils::commit_id_fixture("head")),
            ]
            .into(),
            ..FakeGitClient::default()
        }
    }

    #[test]
    fn artifact_only_mode_renders_an_artifact() {
        let viewer = FakeViewer {
            forward_error: Some("must not forward".into()),
            render: DiffRenderOutcome::Rendered(crate::ports::PlacedArtifact::Created {
                path: crate::utils::absolute_file_path("/tmp/diff.html"),
            }),
        };

        let outcome =
            present_diff::execute(request(DiffPresentationMode::ArtifactOnly), &viewer, &git())
                .unwrap();

        assert_eq!(
            outcome,
            PresentDiffOk::Artifact {
                outcome: DiffRenderOutcome::Rendered(crate::ports::PlacedArtifact::Created {
                    path: crate::utils::absolute_file_path("/tmp/diff.html"),
                }),
                route: ArtifactPresentationRoute::Direct,
                notes: vec![Note::info("rendered")],
            }
        );
    }

    #[test]
    fn graphical_default_uses_the_viewer_surface() {
        let viewer = FakeViewer {
            forward_error: None,
            render: DiffRenderOutcome::Empty,
        };

        let outcome = present_diff::execute(
            request(DiffPresentationMode::ViewerWithArtifactFallback),
            &viewer,
            &git(),
        )
        .unwrap();

        assert_eq!(outcome, PresentDiffOk::Viewer { notes: Vec::new() });
    }

    #[test]
    fn viewer_failure_degrades_to_the_artifact_surface() {
        let viewer = FakeViewer {
            forward_error: Some("viewer unavailable".into()),
            render: DiffRenderOutcome::Rendered(crate::ports::PlacedArtifact::Created {
                path: crate::utils::absolute_file_path("/tmp/diff.html"),
            }),
        };

        let outcome = present_diff::execute(
            request(DiffPresentationMode::ViewerWithArtifactFallback),
            &viewer,
            &git(),
        )
        .unwrap();

        let PresentDiffOk::Artifact {
            outcome,
            route,
            notes,
        } = outcome
        else {
            panic!("viewer failure must fall back to an artifact");
        };
        assert!(matches!(outcome, DiffRenderOutcome::Rendered(_)));
        assert_eq!(
            route,
            ArtifactPresentationRoute::ViewerFallback {
                error: "viewer unavailable".into(),
            }
        );
        assert!(notes[0].text.contains("viewer unavailable"));
    }
}
