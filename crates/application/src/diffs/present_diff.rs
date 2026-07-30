use std::path::{Path, PathBuf};

use crate::{
    diffs::{DiffTarget, PinnedRange},
    ports::{
        DiffRenderOutcome, DiffRenderRequest, DiffViewerBatch, DiffViewerClient, DiffViewerRecipe,
        DiffViewerRecipeOperation, GitClient,
    },
    recipes::RecipeRequest,
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRecipeIntent {
    pub repo: PathBuf,
    pub operation: RecipeRequest,
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
    #[error("failed to resolve recipe repository {repo}: {source}")]
    RecipeRepository {
        repo: PathBuf,
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
) -> Result<DiffViewerBatch, PresentDiffError> {
    let recipes = intents
        .into_iter()
        .map(|intent| build_recipe(intent, git))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DiffViewerBatch { batch_id, recipes })
}

fn build_recipe(
    intent: DiffRecipeIntent,
    git: &impl GitClient,
) -> Result<DiffViewerRecipe, PresentDiffError> {
    let source = git
        .top_level(&intent.repo)
        .map(PathBuf::from)
        .map_err(|source| PresentDiffError::RecipeRepository {
            repo: intent.repo,
            source,
        })?;
    let operation = match intent.operation {
        RecipeRequest::Diff(target) => {
            DiffViewerRecipeOperation::Diff(pin_target(&source, target, git))
        }
        RecipeRequest::MergeDiff { base } => DiffViewerRecipeOperation::MergeDiff {
            pinned: pin_merge(&source, base.as_deref(), git),
            base,
        },
        RecipeRequest::SquashPreview => DiffViewerRecipeOperation::SquashPreview {
            pinned: pin_range(&source, "@{u}", "HEAD", git),
        },
    };
    Ok(DiffViewerRecipe {
        source,
        operation,
        name: intent.name,
    })
}

fn pin_target(repo: &Path, target: DiffTarget, git: &impl GitClient) -> DiffTarget {
    match target {
        DiffTarget::Unpushed { pinned: None } => DiffTarget::Unpushed {
            pinned: pin_range(repo, "@{u}", "HEAD", git),
        },
        DiffTarget::Range {
            range,
            pinned: None,
        } => DiffTarget::Range {
            pinned: pin_exact_range(repo, &range, git),
            range,
        },
        DiffTarget::Merge { base, pinned: None } => DiffTarget::Merge {
            pinned: pin_merge(repo, Some(&base), git),
            base,
        },
        DiffTarget::Last {
            count,
            pinned: None,
        } => DiffTarget::Last {
            pinned: pin_range(repo, &format!("HEAD~{count}"), "HEAD", git),
            count,
        },
        target => target,
    }
}

fn pin_exact_range(repo: &Path, range: &str, git: &impl GitClient) -> Option<PinnedRange> {
    if range.contains("...") {
        return None;
    }
    let (base, head) = range.split_once("..")?;
    if base.is_empty() || head.is_empty() {
        return None;
    }
    pin_range(repo, base, head, git)
}

fn pin_merge(repo: &Path, base: Option<&str>, git: &impl GitClient) -> Option<PinnedRange> {
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::render_merge_diff::DEFAULT_BASE);
    Some(PinnedRange {
        base: git.merge_base(repo, base, "HEAD").ok()?,
        head: git.resolve_sha(repo, "HEAD").ok()?,
    })
}

fn pin_range(repo: &Path, base: &str, head: &str, git: &impl GitClient) -> Option<PinnedRange> {
    Some(PinnedRange {
        base: git.resolve_sha(repo, base).ok()?,
        head: git.resolve_sha(repo, head).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diffs::{DiffTargetRequest, render_diff::RenderDiff},
        ports::{DiffRenderResponse, DiffViewerBatch, DiffViewerClient, DiffViewerRecipeOperation},
        testing::FakeGitClient,
    };

    #[derive(Debug, Clone)]
    struct FakeViewer {
        forward_error: Option<String>,
        render: DiffRenderOutcome,
    }

    impl DiffViewerClient for FakeViewer {
        fn forward(&self, _batch: &DiffViewerBatch) -> anyhow::Result<()> {
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
                repo: "/repo".into(),
                operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
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
    fn graphical_default_forwards_a_pinned_recipe() {
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

    #[test]
    fn recipe_pinning_stays_inside_the_application_operation() {
        let operation = build_recipe(request(false, true, true).recipes.remove(0), &git())
            .expect("recipe should resolve");

        assert_eq!(
            operation.operation,
            DiffViewerRecipeOperation::Diff(DiffTarget::Unpushed {
                pinned: Some(PinnedRange {
                    base: "base".into(),
                    head: "head".into(),
                }),
            })
        );
    }
}
