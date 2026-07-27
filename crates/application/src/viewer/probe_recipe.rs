use contracts::recipes::{Recipe, RecipeSource};

use super::{ViewerTabKind, ViewerTabState};
use crate::{
    live_views::probe::{self, ProbeOutcome, ProbeSource},
    ports::RepoProbe,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecipe {
    pub recipe: Recipe,
    pub kind: ViewerTabKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecipeResponse {
    pub outcome: ProbeRecipeOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeRecipeOutcome {
    Ready,
    Broken { state: ViewerTabState },
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeRecipeError {
    #[error(transparent)]
    Probe(#[from] probe::ProbeSourceError),
}

#[cqrsy::query]
pub fn execute(
    query: ProbeRecipe,
    probe: &impl RepoProbe,
) -> Result<ProbeRecipeResponse, ProbeRecipeError> {
    if query.kind == ViewerTabKind::Snapshot {
        return Ok(ProbeRecipeResponse {
            outcome: ProbeRecipeOutcome::Ready,
        });
    }

    let RecipeSource::LocalRepo(path) = query.recipe.source;
    let response = probe::execute(
        ProbeSource {
            source_kind: "LocalRepo".into(),
            source_value: path.display().to_string(),
        },
        probe,
    )?;
    let outcome = match response.outcome {
        ProbeOutcome::Ok => ProbeRecipeOutcome::Ready,
        ProbeOutcome::Broken { rejection } => ProbeRecipeOutcome::Broken {
            state: ViewerTabState::Broken {
                code: rejection.code().into(),
                reason: rejection.to_string(),
            },
        },
    };

    Ok(ProbeRecipeResponse { outcome })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use contracts::recipes::RecipeOp;

    use super::*;
    use crate::{
        ports::{RepoProbe, RepoProbeResult},
        testing::{FakeRepoProbe, viewer::recipe},
    };

    #[derive(Clone)]
    struct FailingRepoProbe;

    impl RepoProbe for FailingRepoProbe {
        fn probe(&self, _dir: &Path) -> anyhow::Result<RepoProbeResult> {
            anyhow::bail!("probe failed")
        }
    }

    #[test]
    fn snapshot_does_not_probe_its_source() {
        let response = execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::SquashPreview { pinned: None }),
                kind: ViewerTabKind::Snapshot,
            },
            &FakeRepoProbe {
                result: RepoProbeResult::NotFound,
            },
        )
        .expect("snapshot probe policy succeeds");

        assert_eq!(response.outcome, ProbeRecipeOutcome::Ready);
    }

    #[test]
    fn missing_live_source_is_broken() {
        let response = execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::SquashPreview { pinned: None }),
                kind: ViewerTabKind::Live,
            },
            &FakeRepoProbe {
                result: RepoProbeResult::NotFound,
            },
        )
        .expect("live probe succeeds");

        assert_eq!(
            response.outcome,
            ProbeRecipeOutcome::Broken {
                state: ViewerTabState::Broken {
                    code: "DirNotFound".into(),
                    reason: "The git repo's directory at `/repos/project` was not found.".into(),
                },
            }
        );
    }

    #[test]
    fn non_repository_live_source_is_broken() {
        let response = execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::SquashPreview { pinned: None }),
                kind: ViewerTabKind::Live,
            },
            &FakeRepoProbe {
                result: RepoProbeResult::NotAGitRepo,
            },
        )
        .expect("live probe succeeds");

        assert_eq!(
            response.outcome,
            ProbeRecipeOutcome::Broken {
                state: ViewerTabState::Broken {
                    code: "DirNotGitRepo".into(),
                    reason: "The directory `/repos/project` is not a git repository.".into(),
                },
            }
        );
    }

    #[test]
    fn valid_live_source_is_ready() {
        let response = execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::SquashPreview { pinned: None }),
                kind: ViewerTabKind::Live,
            },
            &FakeRepoProbe {
                result: RepoProbeResult::Repo {
                    top_level: "/repos/project".into(),
                },
            },
        )
        .expect("live probe succeeds");

        assert_eq!(response.outcome, ProbeRecipeOutcome::Ready);
    }

    #[test]
    fn unexpected_live_probe_failure_is_returned() {
        let error = execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::SquashPreview { pinned: None }),
                kind: ViewerTabKind::Live,
            },
            &FailingRepoProbe,
        )
        .expect_err("unexpected probe failure returns");

        assert_eq!(error.to_string(), "probe failed");
    }
}
