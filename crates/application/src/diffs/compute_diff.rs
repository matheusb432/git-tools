//! The `compute_diff` vertical slice: resolve a [`DiffTarget`] into the
//! structured [`View`] the native viewer renders — no HTML, no artifact store.
//! The desktop's in-process mediator dispatches this for recipe tabs; the
//! daemon's `--raw` path keeps using `render_diff`.

use std::path::PathBuf;

use domain::diffs::{DiffTarget, View};

use crate::{diffs::render_diff::build_view, ports::DiffSource, shared::notes::Note};

/// Compute the structured diff view for `target`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeDiff {
    pub cwd: PathBuf,
    pub target: DiffTarget,
}

/// The computed view plus the human summary and every surfaced message.
/// An empty view is a legitimate outcome (`view.is_empty()`); tab policy is the
/// caller's concern.
#[derive(Debug, Clone)]
pub struct ComputeDiffResponse {
    pub view: View,
    pub summary: String,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong computing a diff view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeDiffError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Computes a diff by driving the diff engine through its source port.
#[cqrsy::handler(query)]
pub fn handle(
    source: &impl DiffSource,
    req: ComputeDiff,
) -> Result<ComputeDiffResponse, ComputeDiffError> {
    let mut notes = Vec::new();
    let top = source.top_level(&req.cwd)?;
    let (view, summary) = build_view(source, &top, &req.target, None, &mut notes)?;
    Ok(ComputeDiffResponse {
        view,
        summary,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cqrsy::Sender;
    use domain::diffs::{Commit, DiffTarget};

    use super::*;
    use crate::testing::FakeDiffSource;

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

    fn one_commit() -> Commit {
        Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }
    }

    fn req(target: DiffTarget) -> ComputeDiff {
        ComputeDiff {
            cwd: PathBuf::from("/repo"),
            target,
        }
    }

    #[test]
    fn computes_the_view_without_touching_store_or_renderer() {
        let handler = ComputeDiffHandler {
            source: FakeDiffSource {
                top_level: Some("/repo".into()),
                branch: "feature".into(),
                upstream: Some("origin/main".into()),
                commits: vec![one_commit()],
                diff_output: SINGLE_FILE_DIFF.into(),
                ..Default::default()
            },
        };

        let response = handler
            .send_now(req(DiffTarget::Unpushed))
            .expect("compute succeeds");

        assert_eq!(response.view.repo_name, "repo");
        assert_eq!(response.view.branch, "feature");
        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.files[0].path, "f.txt");
        assert_eq!(response.summary, "1 unpushed commit(s)");
        assert!(response.notes.is_empty());
    }

    #[test]
    fn empty_range_returns_an_empty_view_not_an_error() {
        let handler = ComputeDiffHandler {
            source: FakeDiffSource {
                top_level: Some("/repo".into()),
                branch: "feature".into(),
                upstream: Some("origin/main".into()),
                ..Default::default()
            },
        };

        let response = handler
            .send_now(req(DiffTarget::Unpushed))
            .expect("compute succeeds");

        assert!(response.view.is_empty());
    }

    #[test]
    fn no_upstream_falls_back_to_main_with_the_warning_note() {
        let handler = ComputeDiffHandler {
            source: FakeDiffSource {
                top_level: Some("/repo".into()),
                branch: "feature".into(),
                upstream: None,
                known_revs: vec!["main".into()],
                commits: vec![one_commit()],
                diff_output: SINGLE_FILE_DIFF.into(),
                ..Default::default()
            },
        };

        let response = handler
            .send_now(req(DiffTarget::Unpushed))
            .expect("compute succeeds");

        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff-preview: no upstream; falling back to main"
            )]
        );
    }

    #[test]
    fn unknown_base_is_an_error() {
        let handler = ComputeDiffHandler {
            source: FakeDiffSource {
                top_level: Some("/repo".into()),
                branch: "feature".into(),
                known_revs: vec![],
                ..Default::default()
            },
        };

        let error = handler
            .send_now(req(DiffTarget::Base("nope".into())))
            .expect_err("unknown base errors");

        let ComputeDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
