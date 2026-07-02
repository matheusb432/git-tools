use std::path::PathBuf;

use anyhow::Context as _;
use contracts::{
    diffs::{DiffTargetDto, RenderDiffRequest},
    envelope::{NoteLevel, Outcome},
};

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
        target: to_target_dto(target),
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
        Outcome::Error => {
            // The CLI's exit path prints `{err:#}` to stderr — hand it the
            // service-composed error text so output stays byte-identical.
            let text = envelope
                .notes
                .iter()
                .rev()
                .find(|n| n.level == NoteLevel::Error)
                .map_or_else(
                    || "daemon reported an error".to_string(),
                    |n| n.text.clone(),
                );
            Err(anyhow::anyhow!(text))
        }
    }
}

/// Map a [`DiffTarget`] onto its wire DTO.
fn to_target_dto(target: &DiffTarget) -> DiffTargetDto {
    match target {
        DiffTarget::Unpushed => DiffTargetDto::Unpushed,
        DiffTarget::Base(rev) => DiffTargetDto::Base { rev: rev.clone() },
        DiffTarget::Range(range) => DiffTargetDto::Range {
            range: range.clone(),
        },
        DiffTarget::Merge(base) => DiffTargetDto::Merge { base: base.clone() },
        DiffTarget::Last(count) => DiffTargetDto::Last { count: count.get() },
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use contracts::{
        diffs::RenderDiffData,
        envelope::{Envelope, Note, Outcome},
    };

    use super::*;

    #[test]
    fn to_target_dto_maps_every_variant() {
        assert_eq!(
            to_target_dto(&DiffTarget::Unpushed),
            DiffTargetDto::Unpushed
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Base("abc".into())),
            DiffTargetDto::Base { rev: "abc".into() }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Range("a..b".into())),
            DiffTargetDto::Range {
                range: "a..b".into()
            }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Merge("main".into())),
            DiffTargetDto::Merge {
                base: "main".into()
            }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Last(NonZeroU32::new(3).unwrap())),
            DiffTargetDto::Last { count: 3 }
        );
    }

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
        let err = match run_with(&backend, &DiffTarget::Unpushed, None) {
            Err(err) => err,
            Ok(_) => panic!("error outcome must map to Err"),
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
