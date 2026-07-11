use std::path::{Path, PathBuf};

use anyhow::Context as _;
use contracts::{diffs::RenderDiffRequest, envelope::Outcome};

use crate::{cli::DiffTarget, client::Backend, viewer};

/// Outcome of a single `diff`/`diff merge`/`diff squash` invocation: either an artifact
/// was written, or the range was empty and we deliberately skipped rendering a blank
/// preview (`diff` only).
pub enum DiffOutcome {
    Rendered(PathBuf),
    Empty,
}

/// Decide whether this invocation opens the artifact in the browser (`--raw`) rather
/// than the desktop viewer: either `--raw` was passed explicitly, or there is no display
/// to spawn a viewer on (the headless degrade). Pure, so the routing decision is
/// unit-testable without an env var.
pub(crate) fn take_raw_path(raw: bool, has_display: bool) -> bool {
    raw || !has_display
}

/// Render `target` through the daemon and open the resulting store artifact: the
/// `--raw`/headless path opens it in the browser ([`super::open_artifact`]); the default
/// path hands it to the desktop viewer as a `diff://` url ([`super::open_in_viewer`]),
/// degrading to the browser when the viewer can't be launched (FSD A-0002 — a
/// header-less machine still renders).
pub fn run(target: &DiffTarget, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let open: fn(&Path) = if take_raw_path(raw, viewer::has_display()) {
        super::open_artifact
    } else {
        super::open_in_viewer
    };
    let backend = crate::client::HttpBackend::ensure_daemon()?;
    render(&backend, target, name, open)
}

/// Render `target` through `backend`, printing its wire notes and handing the artifact
/// to `open`. Split from [`run`] so tests can drive a fake backend.
pub(crate) fn render(
    backend: &impl Backend,
    target: &DiffTarget,
    name: Option<&str>,
    open: impl FnOnce(&Path),
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
            open(&artifact);
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
    fn take_raw_path_is_true_for_explicit_raw_or_no_display() {
        assert!(take_raw_path(true, true));
        assert!(take_raw_path(true, false));
        assert!(take_raw_path(false, false));
        assert!(!take_raw_path(false, true));
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
        let Err(err) = render(&backend, &DiffTarget::Unpushed, None, |_| {}) else {
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
            render(&backend, &DiffTarget::Unpushed, None, |_| {}).unwrap(),
            DiffOutcome::Empty
        ));
    }
}
