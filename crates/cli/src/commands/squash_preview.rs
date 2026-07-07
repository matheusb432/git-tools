use std::path::{Path, PathBuf};

use contracts::diffs::RenderSquashPreviewRequest;

use crate::client::Backend;

pub fn run(repo: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    run_with(&backend, repo)
}

/// Render `repo`'s squash-preview through `backend`, printing its wire notes
/// and opening the artifact. Split from [`run`] so tests can drive a fake
/// backend.
pub(crate) fn run_with(backend: &impl Backend, repo: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    // Lexical, no filesystem access — the daemon process's own cwd is unrelated
    // to the caller's shell, so `cwd` must already be absolute on the wire.
    let cwd = std::path::absolute(repo.as_ref())?;
    let req = RenderSquashPreviewRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
    };
    super::finish_single_render(backend.render_squash_preview(&req)?)
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
        fn render_squash_preview(
            &self,
            _req: &RenderSquashPreviewRequest,
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
        let Err(err) = run_with(&backend, ".") else {
            panic!("error outcome must map to Err")
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }
}
