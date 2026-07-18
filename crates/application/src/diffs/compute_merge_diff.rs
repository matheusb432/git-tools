//! The `compute_merge_diff` vertical slice: the structured merge [`View`] for
//! the native viewer — no HTML, no artifact store.

use std::path::PathBuf;

use crate::{
    diffs::{PinnedRange, View, render_merge_diff::build_merge_view},
    ports::{DiffSource, UserSettingsStore},
};

/// Compute the merge view of the current branch into `base` (default `main`),
/// resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeMergeDiff {
    pub cwd: PathBuf,
    pub base: Option<String>,
    pub pinned: Option<PinnedRange>,
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
#[cqrsy::query]
pub fn execute(
    req: ComputeMergeDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl DiffSource,
) -> Result<ComputeMergeDiffResponse, ComputeMergeDiffError> {
    let ComputeMergeDiff { cwd, base, pinned } = req;
    let settings = app_settings.load();
    let built = build_merge_view(
        source,
        &cwd,
        base.as_deref(),
        pinned.as_ref(),
        None,
        settings.diff_exclusions(),
    )?;
    Ok(ComputeMergeDiffResponse { view: built.view })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::DiffExclusions;

    use super::*;
    use crate::{
        ports::AppSettings,
        testing::{
            FakeDiffSource, FixedUserSettingsStore,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    const CODE_AND_NOTES_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n\
diff --git a/docs/notes.md b/docs/notes.md\n\
index 333..444 100644\n\
--- a/docs/notes.md\n\
+++ b/docs/notes.md\n\
@@ -1 +1 @@\n\
-plan\n\
+more plan\n";

    fn execute_default_settings(
        request: ComputeMergeDiff,
        source: &FakeDiffSource,
    ) -> Result<ComputeMergeDiffResponse, ComputeMergeDiffError> {
        execute(request, &FixedUserSettingsStore::default(), source)
    }

    #[test]
    fn app_settings_exclusions_apply_to_the_resolved_repository() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let app_settings = FixedUserSettingsStore::new(AppSettings::new(
            None,
            true,
            DiffExclusions::new([("repo".into(), vec!["md"])], None),
        ));

        let response = execute(
            ComputeMergeDiff {
                cwd: PathBuf::from("/repo"),
                base: None,
                pinned: None,
            },
            &app_settings,
            &source,
        )
        .expect("compute succeeds");

        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.files[0].path, "f.txt");
        assert_eq!(
            response
                .view
                .exclusions
                .expect("exclusion summary")
                .hidden_paths,
            ["docs/notes.md"]
        );
    }

    #[test]
    fn computes_the_merge_view_with_the_default_base() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            ComputeMergeDiff {
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
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            ComputeMergeDiff {
                cwd: PathBuf::from("/repo"),
                base: None,
                pinned: Some(PinnedRange {
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

        let error = execute_default_settings(
            ComputeMergeDiff {
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
