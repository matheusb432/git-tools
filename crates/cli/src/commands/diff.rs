use std::path::PathBuf;

use anyhow::Context as _;
use contracts::{diffs::RenderDiffRequest, envelope::Outcome};

use crate::{cli::DiffTarget, client::Backend};

/// Outcome of a single `diff` invocation: either an artifact was written, or the
/// range was empty and we deliberately skipped rendering a blank preview.
pub enum DiffOutcome {
    Rendered(PathBuf),
    Empty,
}

pub fn run(target: &DiffTarget, name: Option<&str>) -> anyhow::Result<DiffOutcome> {
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_with(&backend, target, name)
}

/// Render `target` through `backend`, printing its wire notes and opening the
/// artifact. Split from [`run`] so tests can drive a fake backend.
pub(crate) fn run_with(
    backend: &impl Backend,
    target: &DiffTarget,
    name: Option<&str>,
) -> anyhow::Result<DiffOutcome> {
    let req = RenderDiffRequest {
        cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        target: super::to_target_dto(target),
        name: name.map(str::to_string),
        theme: crate::config::load().theme,
    };
    let envelope = backend.render_diff(&req)?;
    super::print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            super::open_artifact(&artifact);
            Ok(DiffOutcome::Rendered(artifact))
        }
        Outcome::Empty => Ok(DiffOutcome::Empty),
        Outcome::Error => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
    }
}

#[cfg(test)]
mod tests {
    use contracts::{
        diffs::RenderDiffData,
        envelope::{Envelope, Note, NoteLevel, Outcome},
    };

    use super::*;

    struct FakeBackend(Envelope<RenderDiffData>);

    impl Backend for FakeBackend {
        fn render_diff(
            &self,
            _req: &RenderDiffRequest,
        ) -> anyhow::Result<Envelope<RenderDiffData>> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn error_outcome_surfaces_the_error_note_text() {
        let backend = FakeBackend(Envelope {
            outcome: Outcome::Error,
            notes: vec![Note {
                level: NoteLevel::Error,
                text: "not a git repo".into(),
            }],
            data: None,
        });
        let Err(err) = run_with(&backend, &DiffTarget::Unpushed, None) else {
            panic!("error outcome must map to Err")
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }

    #[test]
    fn empty_outcome_maps_to_empty() {
        let backend = FakeBackend(Envelope {
            outcome: Outcome::Empty,
            notes: vec![],
            data: None,
        });
        assert!(matches!(
            run_with(&backend, &DiffTarget::Unpushed, None).unwrap(),
            DiffOutcome::Empty
        ));
    }
}
