//! The `compute_merge_diff` vertical slice: the structured merge [`View`] for
//! the native viewer — no HTML, no artifact store.

use std::path::PathBuf;

use domain::diffs::{DiffExclusions, View};

use crate::{diffs::render_merge_diff::build_merge_view, ports::DiffSource};

/// Compute the merge view of the current branch into `base` (default `main`),
/// resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeMergeDiff {
    pub cwd: PathBuf,
    pub base: Option<String>,
    pub pinned: Option<domain::diffs::PinnedRange>,
    pub exclusions: DiffExclusions,
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

/// Computes a merge diff via the shared merge view builder.
#[cqrsy::handler(query)]
pub fn execute(
    req: ComputeMergeDiff,
    source: &impl DiffSource,
) -> Result<ComputeMergeDiffResponse, ComputeMergeDiffError> {
    let ComputeMergeDiff {
        cwd,
        base,
        pinned,
        exclusions,
    } = req;
    let built = build_merge_view(source, &cwd, base.as_deref(), pinned.as_ref(), &exclusions)?;
    Ok(ComputeMergeDiffResponse { view: built.view })
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
    fn computes_the_merge_view_with_the_default_base() {
        let source = FakeDiffSource {
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
        };

        let response = execute(
            ComputeMergeDiff {
                exclusions: DiffExclusions::default(),
                cwd: PathBuf::from("/repo"),
                base: None,
                pinned: None,
            },
            &source,
        )
        .expect("compute succeeds");

        assert_eq!(response.view.upstream, "main");
        assert_eq!(response.view.branch, "feature");
        assert_eq!(response.view.files.len(), 1);
    }

    #[test]
    fn pinned_merge_diff_computes_over_the_pinned_range_without_verification() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![], // verify_commit would fail symbolically
            commits: vec![Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                ..Default::default()
            }],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response = execute(
            ComputeMergeDiff {
                exclusions: DiffExclusions::default(),
                cwd: PathBuf::from("/repo"),
                base: None,
                pinned: Some(domain::diffs::PinnedRange {
                    base: "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd".into(),
                    head: "1111111111222222222233333333334444444444".into(),
                }),
            },
            &source,
        )
        .expect("pinned merge compute succeeds");

        assert_eq!(response.view.title, "merge-diff");
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };

        let error = execute(
            ComputeMergeDiff {
                exclusions: DiffExclusions::default(),
                cwd: PathBuf::from("/repo"),
                base: Some("nope".into()),
                pinned: None,
            },
            &source,
        )
        .expect_err("unknown base errors");

        let ComputeMergeDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
