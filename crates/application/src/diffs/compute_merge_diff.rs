//! The `compute_merge_diff` vertical slice: the structured merge [`View`] for
//! the native viewer — no HTML, no artifact store.

use std::path::PathBuf;

use cqrsy::Handler;
use domain::diffs::View;

use crate::{diffs::render_merge_diff::build_merge_view, ports::DiffSource};

/// Compute the merge view of the current branch into `base` (default `main`),
/// resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq, cqrsy::Query)]
#[query(out = ComputeMergeDiffResponse, err = ComputeMergeDiffError)]
pub struct ComputeMergeDiff {
    pub cwd: PathBuf,
    pub base: Option<String>,
}

/// The computed merge view.
#[derive(Debug, Clone)]
pub struct ComputeMergeDiffResponse {
    pub view: View,
}

/// Everything that can go wrong computing a merge view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeMergeDiffError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Handles [`ComputeMergeDiff`] via the shared merge view builder.
#[derive(Clone)]
pub struct ComputeMergeDiffHandler<S: DiffSource> {
    pub source: S,
}

impl<S: DiffSource> Handler<ComputeMergeDiff> for ComputeMergeDiffHandler<S> {
    async fn handle(
        &self,
        req: ComputeMergeDiff,
    ) -> Result<ComputeMergeDiffResponse, ComputeMergeDiffError> {
        let built = build_merge_view(&self.source, &req.cwd, req.base.as_deref())?;
        Ok(ComputeMergeDiffResponse { view: built.view })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cqrsy::send_now;
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
    fn computes_the_merge_view_with_the_default_base() {
        let handler = ComputeMergeDiffHandler {
            source: FakeDiffSource {
                top_level: Some("/repo".into()),
                branch: "feature".into(),
                known_revs: vec!["main".into()],
                commits: vec![Commit {
                    sha: "abc1234".into(),
                    subject: "feat: work".into(),
                    ..Default::default()
                }],
                diff_output: SINGLE_FILE_DIFF.into(),
                ..Default::default()
            },
        };

        let response = send_now(
            &(),
            &handler,
            ComputeMergeDiff {
                cwd: PathBuf::from("/repo"),
                base: None,
            },
        )
        .expect("compute succeeds");

        assert_eq!(response.view.upstream, "main");
        assert_eq!(response.view.branch, "feature");
        assert_eq!(response.view.files.len(), 1);
    }

    #[test]
    fn unknown_base_is_an_error() {
        let handler = ComputeMergeDiffHandler {
            source: FakeDiffSource {
                top_level: Some("/repo".into()),
                branch: "feature".into(),
                known_revs: vec![],
                ..Default::default()
            },
        };

        let error = send_now(
            &(),
            &handler,
            ComputeMergeDiff {
                cwd: PathBuf::from("/repo"),
                base: Some("nope".into()),
            },
        )
        .expect_err("unknown base errors");

        let ComputeMergeDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
