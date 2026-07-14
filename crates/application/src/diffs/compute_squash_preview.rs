//! The `compute_squash_preview` vertical slice: the structured squash-preview
//! [`View`] for the native viewer — no HTML, no artifact store.

use std::path::PathBuf;

use domain::diffs::View;

use crate::{diffs::render_squash_preview::build_squash_view, ports::DiffSource};

/// Compute the squash-preview view of the current branch's unpushed commits
/// (base is always the configured upstream), resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeSquashPreview {
    pub cwd: PathBuf,
    pub pinned: Option<domain::diffs::PinnedRange>,
}

/// The computed squash-preview view.
#[derive(Debug, Clone)]
pub struct ComputeSquashPreviewResponse {
    pub view: View,
}

/// Everything that can go wrong computing a squash-preview view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeSquashPreviewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Computes a squash preview via the shared squash view builder.
#[cqrsy::handler(query)]
pub fn execute(
    req: ComputeSquashPreview,
    source: &impl DiffSource,
) -> Result<ComputeSquashPreviewResponse, ComputeSquashPreviewError> {
    let ComputeSquashPreview { cwd, pinned } = req;
    let built = build_squash_view(source, &cwd, pinned.as_ref())?;
    Ok(ComputeSquashPreviewResponse { view: built.view })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::Commit;

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

    #[test]
    fn computes_the_squash_view_from_the_upstream() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                ..Default::default()
            }],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response = execute(
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
                pinned: None,
            },
            &source,
        )
        .expect("compute succeeds");

        assert_eq!(response.view.title, "squash-preview");
        assert_eq!(response.view.upstream, "origin/main");
        assert_eq!(response.view.files.len(), 1);
    }

    #[test]
    fn pinned_squash_preview_computes_without_an_upstream() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None, // symbolic squash preview errors with "no upstream"
            commits: vec![Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                ..Default::default()
            }],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response = execute(
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
                pinned: Some(domain::diffs::PinnedRange {
                    base: "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd".into(),
                    head: "1111111111222222222233333333334444444444".into(),
                }),
            },
            &source,
        )
        .expect("pinned squash compute succeeds");

        assert_eq!(response.view.upstream, "aaaaaaaaaa");
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
    }

    #[test]
    fn missing_upstream_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            ..Default::default()
        };

        let error = execute(
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
                pinned: None,
            },
            &source,
        )
        .expect_err("no upstream errors");

        let ComputeSquashPreviewError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "no upstream");
    }
}
