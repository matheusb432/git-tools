use std::path::Path;

use contracts::diffs::RenderSquashPreviewRequest;

use crate::{
    client::Backend,
    commands::diff::{DiffOutcome, take_raw_path},
    viewer,
};

/// Render `repo`'s squash-preview through the daemon and open the resulting store
/// artifact: the `--raw`/headless path opens it in the browser; the default path hands it
/// to the desktop viewer as a `diff://` url, degrading to the browser when the viewer
/// can't be launched (FSD A-0002 — a header-less machine still renders).
pub fn run(repo: impl AsRef<Path>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let open: fn(&Path) = if take_raw_path(raw, viewer::has_display()) {
        super::open_artifact
    } else {
        super::open_in_viewer
    };
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    render(&backend, repo, open)
}

/// Render `repo`'s squash-preview through `backend`, printing its wire notes and handing
/// the artifact to `open`. Split from [`run`] so tests can drive a fake backend.
pub(crate) fn render(
    backend: &impl Backend,
    repo: impl AsRef<Path>,
    open: impl FnOnce(&Path),
) -> anyhow::Result<DiffOutcome> {
    // Lexical, no filesystem access — the daemon process's own cwd is unrelated
    // to the caller's shell, so `cwd` must already be absolute on the wire.
    let cwd = std::path::absolute(repo.as_ref())?;
    let req = RenderSquashPreviewRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        store_root: gtl_platform::paths::store_root()?
            .to_string_lossy()
            .into_owned(),
    };
    let artifact = super::finish_single_render(backend.render_squash_preview(&req)?, open)?;
    Ok(DiffOutcome::Rendered(artifact))
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
        let Err(err) = render(&backend, ".", |_| {}) else {
            panic!("error outcome must map to Err")
        };
        assert_eq!(format!("{err:#}"), "not a git repo");
    }
}
