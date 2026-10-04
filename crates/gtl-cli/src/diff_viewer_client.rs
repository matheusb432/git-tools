use std::path::PathBuf;

use anyhow::Context as _;
use gtl_models::paths::AbsoluteFilePath;
use gtl_wire::v1;

use crate::commands::diff::DiffOutcome;

pub(crate) struct PresentedDiffResult {
    presentation: Option<v1::DiffPresentation>,
}

pub(crate) struct RenderedDiffResult {
    notes: Vec<v1::Note>,
    outcome: Option<RenderedDiffOutcome>,
}

enum RenderedDiffOutcome {
    Rendered(v1::Artifact),
    Empty,
}

pub(crate) fn finish_presentation(
    response: impl Into<PresentedDiffResult>,
) -> anyhow::Result<DiffOutcome> {
    let presentation = response
        .into()
        .presentation
        .context("gtl-server returned no viewer presentation")?;
    print_notes(&presentation.notes)?;
    match presentation
        .outcome
        .context("gtl-server returned no viewer presentation outcome")?
    {
        v1::diff_presentation::Outcome::ViewerOpened(_)
        | v1::diff_presentation::Outcome::ViewerUnavailable(_) => Ok(DiffOutcome::Forwarded),
        v1::diff_presentation::Outcome::Rendered(artifact) => finish_artifact(artifact),
        v1::diff_presentation::Outcome::Empty(_) => Ok(DiffOutcome::Empty),
    }
}

pub(crate) fn finish_render(
    response: impl Into<RenderedDiffResult>,
) -> anyhow::Result<DiffOutcome> {
    let response = response.into();
    print_notes(&response.notes)?;
    match response
        .outcome
        .context("gtl-server returned no diff outcome")?
    {
        RenderedDiffOutcome::Rendered(artifact) => finish_artifact(artifact),
        RenderedDiffOutcome::Empty => Ok(DiffOutcome::Empty),
    }
}

fn finish_artifact(artifact: v1::Artifact) -> anyhow::Result<DiffOutcome> {
    let path = AbsoluteFilePath::try_new(PathBuf::from(artifact.path))
        .context("gtl-server returned a non-absolute artifact path")?;
    match v1::ArtifactPlacement::try_from(artifact.placement) {
        Ok(v1::ArtifactPlacement::Created | v1::ArtifactPlacement::Reused) => {}
        Ok(v1::ArtifactPlacement::Unspecified) | Err(_) => {
            anyhow::bail!("gtl-server returned an invalid artifact placement")
        }
    }
    println!("{}", crate::commands::file_url(path.as_path()));
    Ok(DiffOutcome::Rendered(path))
}

macro_rules! presentation_conversion {
    ($response:ty) => {
        impl From<$response> for PresentedDiffResult {
            fn from(response: $response) -> Self {
                Self {
                    presentation: response.presentation,
                }
            }
        }
    };
}

presentation_conversion!(v1::PresentDiffResponse);
presentation_conversion!(v1::PresentMergeDiffResponse);
presentation_conversion!(v1::PresentSubrepositoryDiffsResponse);
presentation_conversion!(v1::PresentProjectRepositoryDiffsResponse);
presentation_conversion!(v1::PresentTextDiffResponse);

impl From<v1::RenderDiffResponse> for RenderedDiffResult {
    fn from(response: v1::RenderDiffResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_diff_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_diff_response::Outcome::Empty(_) => RenderedDiffOutcome::Empty,
            }),
        }
    }
}

impl From<v1::RenderTextDiffResponse> for RenderedDiffResult {
    fn from(response: v1::RenderTextDiffResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.rendered.map(RenderedDiffOutcome::Rendered),
        }
    }
}

impl From<v1::RenderMergeDiffResponse> for RenderedDiffResult {
    fn from(response: v1::RenderMergeDiffResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_merge_diff_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_merge_diff_response::Outcome::Empty(_) => RenderedDiffOutcome::Empty,
            }),
        }
    }
}

impl From<v1::RenderSubrepositoryDiffsResponse> for RenderedDiffResult {
    fn from(response: v1::RenderSubrepositoryDiffsResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_subrepository_diffs_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_subrepository_diffs_response::Outcome::Empty(_) => {
                    RenderedDiffOutcome::Empty
                }
            }),
        }
    }
}

impl From<v1::RenderProjectRepositoryDiffsResponse> for RenderedDiffResult {
    fn from(response: v1::RenderProjectRepositoryDiffsResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_project_repository_diffs_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_project_repository_diffs_response::Outcome::Empty(_) => {
                    RenderedDiffOutcome::Empty
                }
            }),
        }
    }
}

fn print_notes(notes: &[v1::Note]) -> anyhow::Result<()> {
    for note in notes {
        match v1::NoteLevel::try_from(note.level) {
            Ok(v1::NoteLevel::Info) => {}
            Ok(v1::NoteLevel::Warning) => eprintln!("{}", note.text),
            Ok(v1::NoteLevel::Error) => anyhow::bail!(note.text.clone()),
            Ok(v1::NoteLevel::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid diff note level")
            }
        }
    }
    Ok(())
}
