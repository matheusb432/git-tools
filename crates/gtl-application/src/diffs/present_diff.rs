use std::path::PathBuf;

use gtl_wire::recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp};

use crate::{
    ports::{DiffRenderOutcome, DiffRenderRequest, DiffViewerClient, GitClient},
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRecipeIntent {
    pub repo_path: PathBuf,
    pub operation: RecipeOp,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentDiff {
    pub render: DiffRenderRequest,
    pub batch_id: String,
    pub recipes: Vec<DiffRecipeIntent>,
    pub raw: bool,
    pub has_display: bool,
    pub effects_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSurface {
    Viewer,
    Artifact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentDiffOk {
    pub surface: DiffSurface,
    pub artifact: Option<PathBuf>,
    pub notes: Vec<Note>,
    pub degraded: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PresentDiffError {
    #[error("failed to resolve recipe repository {repo_path}: {source}")]
    RecipeRepository {
        repo_path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
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
        return Ok(PresentDiffOk {
            surface: DiffSurface::Artifact,
            artifact: None,
            notes: Vec::new(),
            degraded: false,
        });
    }
    let render_directly = command.raw || !command.has_display || !command.effects_enabled;
    if render_directly {
        return render_artifact(&command.render, false, viewer);
    }

    let batch = build_batch(command.batch_id, command.recipes, git)?;
    match viewer.forward(&batch) {
        Ok(()) => Ok(PresentDiffOk {
            surface: DiffSurface::Viewer,
            artifact: None,
            notes: Vec::new(),
            degraded: false,
        }),
        Err(error) => {
            let mut outcome = render_artifact(&command.render, true, viewer)?;
            outcome.notes.insert(
                0,
                Note::warn(format!(
                    "diff: viewer unavailable ({error:#}); rendering an artifact instead"
                )),
            );
            Ok(outcome)
        }
    }
}

fn render_artifact(
    request: &DiffRenderRequest,
    degraded: bool,
    viewer: &impl DiffViewerClient,
) -> Result<PresentDiffOk, PresentDiffError> {
    let response = viewer.render(request)?;
    let artifact = match response.outcome {
        DiffRenderOutcome::Rendered(path) => Some(path),
        DiffRenderOutcome::Empty => None,
    };
    Ok(PresentDiffOk {
        surface: DiffSurface::Artifact,
        artifact,
        notes: response.notes,
        degraded,
    })
}

fn build_batch(
    batch_id: String,
    intents: Vec<DiffRecipeIntent>,
    git: &impl GitClient,
) -> Result<OpenRecipes, PresentDiffError> {
    let recipes = intents
        .into_iter()
        .map(|intent| build_recipe(intent, git))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OpenRecipes {
        batch_id,
        kind: RecipeBatchKind::Snapshot,
        recipes,
    })
}

fn build_recipe(
    intent: DiffRecipeIntent,
    git: &impl GitClient,
) -> Result<Recipe, PresentDiffError> {
    let source = git
        .top_level(&intent.repo_path)
        .map(PathBuf::from)
        .map_err(|source| PresentDiffError::RecipeRepository {
            repo_path: intent.repo_path,
            source,
        })?;
    Ok(crate::recipes::build_resolved(
        source,
        intent.operation,
        intent.name,
        git,
    ))
}

#[cfg(test)]
mod tests {
    use gtl_wire::recipes::{OpenRecipes, RecipeTarget};

    use super::*;
    use crate::{
        diffs::{DiffTargetRequest, render_diff::RenderDiff},
        ports::{DiffRenderResponse, DiffViewerClient},
        testing::FakeGitClient,
    };

    #[derive(Debug, Clone)]
    struct FakeViewer {
        forward_error: Option<String>,
        render: DiffRenderOutcome,
    }

    impl DiffViewerClient for FakeViewer {
        fn forward(&self, _batch: &OpenRecipes) -> anyhow::Result<()> {
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

    fn request(raw: bool, has_display: bool, effects_enabled: bool) -> PresentDiff {
        PresentDiff {
            render: DiffRenderRequest::Diff(RenderDiff {
                cwd: "/repo".into(),
                target: DiffTargetRequest::Unpushed,
                name: None,
            }),
            batch_id: "batch".into(),
            recipes: vec![DiffRecipeIntent {
                repo_path: "/repo".into(),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: None,
            }],
            raw,
            has_display,
            effects_enabled,
        }
    }

    fn git() -> FakeGitClient {
        FakeGitClient {
            top_level: Some("/repo".into()),
            shas: [
                ("@{u}".to_string(), "base".to_string()),
                ("HEAD".to_string(), "head".to_string()),
            ]
            .into(),
            ..FakeGitClient::default()
        }
    }

    #[test]
    fn disabled_effects_render_an_artifact() {
        let viewer = FakeViewer {
            forward_error: Some("must not forward".into()),
            render: DiffRenderOutcome::Rendered("/tmp/diff.html".into()),
        };

        let outcome = execute(request(false, true, false), &viewer, &git()).unwrap();

        assert_eq!(outcome.surface, DiffSurface::Artifact);
        assert_eq!(outcome.artifact, Some("/tmp/diff.html".into()));
        assert!(!outcome.degraded);
        assert_eq!(outcome.notes, vec![Note::info("rendered")]);
    }

    #[test]
    fn graphical_default_uses_the_viewer_surface() {
        let viewer = FakeViewer {
            forward_error: None,
            render: DiffRenderOutcome::Empty,
        };

        let outcome = execute(request(false, true, true), &viewer, &git()).unwrap();

        assert_eq!(outcome.surface, DiffSurface::Viewer);
        assert_eq!(outcome.artifact, None);
        assert!(!outcome.degraded);
    }

    #[test]
    fn viewer_failure_degrades_to_the_artifact_surface() {
        let viewer = FakeViewer {
            forward_error: Some("viewer unavailable".into()),
            render: DiffRenderOutcome::Rendered("/tmp/diff.html".into()),
        };

        let outcome = execute(request(false, true, true), &viewer, &git()).unwrap();

        assert_eq!(outcome.surface, DiffSurface::Artifact);
        assert!(outcome.degraded);
        assert_eq!(outcome.artifact, Some("/tmp/diff.html".into()));
        assert!(outcome.notes[0].text.contains("viewer unavailable"));
    }

    #[test]
    fn raw_renders_an_artifact() {
        let viewer = FakeViewer {
            forward_error: Some("must not forward".into()),
            render: DiffRenderOutcome::Rendered("/tmp/diff.html".into()),
        };

        let outcome = execute(request(true, true, true), &viewer, &git()).unwrap();

        assert_eq!(outcome.surface, DiffSurface::Artifact);
        assert_eq!(outcome.artifact, Some("/tmp/diff.html".into()));
        assert_eq!(outcome.notes, vec![Note::info("rendered")]);
    }
}
