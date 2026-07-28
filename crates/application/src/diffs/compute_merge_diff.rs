//! The `compute_merge_diff` vertical slice: the structured merge [`View`] for
//! the native viewer — no HTML, no artifact store.

mod view;

use std::path::PathBuf;

pub(crate) use view::MergeViewBuild;

use crate::{
    diffs::{PinnedRange, View},
    ports::{AppSettings, GitClient, UserSettingsStore},
};

/// Falls back to this base when a merge request omits `base` or supplies a blank value.
///
/// # Examples
///
/// ```
/// use application::diffs::compute_merge_diff::DEFAULT_BASE;
///
/// assert_eq!(DEFAULT_BASE, "main");
/// ```
pub const DEFAULT_BASE: &str = "main";

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
pub struct ComputeMergeDiffOk {
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
    source: &impl GitClient,
) -> Result<ComputeMergeDiffOk, ComputeMergeDiffError> {
    let settings = app_settings.load();
    let built = compute(req, &settings, source)?;
    Ok(ComputeMergeDiffOk { view: built.view })
}

pub(crate) fn compute(
    req: ComputeMergeDiff,
    settings: &AppSettings,
    source: &impl GitClient,
) -> anyhow::Result<MergeViewBuild> {
    let ComputeMergeDiff { cwd, base, pinned } = req;
    view::build(
        source,
        &cwd,
        base.as_deref(),
        pinned.as_ref(),
        settings.diff_exclusions(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::DiffExclusions;

    use super::*;
    use crate::{
        ports::AppSettings,
        testing::{
            FakeGitClient, FixedUserSettingsStore,
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
        source: &FakeGitClient,
    ) -> Result<ComputeMergeDiffOk, ComputeMergeDiffError> {
        execute(request, &FixedUserSettingsStore::default(), source)
    }

    #[test]
    fn viewer_and_raw_computation_share_the_same_view() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let settings = AppSettings::default();
        let request = ComputeMergeDiff {
            cwd: PathBuf::from("/repo"),
            base: None,
            pinned: None,
        };

        let viewer = execute(
            request.clone(),
            &FixedUserSettingsStore::new(settings.clone()),
            &source,
        )
        .expect("viewer compute succeeds");
        let raw = compute(request, &settings, &source).expect("raw compute succeeds");

        assert_eq!(viewer.view, raw.view);
    }

    #[test]
    fn app_settings_exclusions_apply_to_the_resolved_repository() {
        let source = FakeGitClient {
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
        let source = FakeGitClient {
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
        let source = FakeGitClient {
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
        let source = FakeGitClient {
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
