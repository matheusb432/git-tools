use std::path::{Path, PathBuf};

use anyhow::Context as _;
use contracts::{diffs::RenderMergeDiffRequest, envelope::Outcome};

use crate::client::Backend;

pub fn run(repo: impl AsRef<Path>, base: Option<&str>) -> anyhow::Result<PathBuf> {
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_with(&backend, repo, base)
}

/// Render `repo`'s merge-diff into `base` through `backend`, printing its wire
/// notes and opening the artifact. Split from [`run`] so tests can drive a fake
/// backend.
pub(crate) fn run_with(
    backend: &impl Backend,
    repo: impl AsRef<Path>,
    base: Option<&str>,
) -> anyhow::Result<PathBuf> {
    // Lexical, no filesystem access — the daemon process's own cwd is unrelated
    // to the caller's shell, so `cwd` must already be absolute on the wire.
    let cwd = std::path::absolute(repo.as_ref())?;
    let req = RenderMergeDiffRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
        base: base.map(str::to_string),
    };
    let envelope = backend.render_merge_diff(&req)?;
    super::print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            super::open_artifact(&artifact);
            Ok(artifact)
        }
        _ => Err(anyhow::anyhow!(super::error_text(&envelope.notes))),
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
        fn render_merge_diff(
            &self,
            _req: &RenderMergeDiffRequest,
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
        let err = match run_with(&backend, ".", None) {
            Err(err) => err,
            Ok(_) => panic!("error outcome must map to Err"),
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }
}
