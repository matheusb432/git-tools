//! The `compute_merge_diff` vertical slice: the structured merge [`View`] for
//! the native viewer — no HTML, no artifact store.

mod view;

use std::path::PathBuf;

use crate::{
    diffs::{PinnedRange, View},
    ports::{GitClient, UserSettingsLoadError, UserSettingsStore},
};

/// Falls back to this base when a merge request omits `base` or supplies a blank value.
///
/// # Examples
///
/// ```
/// use gtl_application::diffs::compute_merge_diff::DEFAULT_BASE;
///
/// assert_eq!(DEFAULT_BASE, "main");
/// ```
pub const DEFAULT_BASE: &str = crate::shared::git_range_pinning::DEFAULT_MERGE_BASE;

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
    pub top: String,
    pub base: String,
    pub diff_range: String,
    pub render_options: gtl_models::viewer::RenderOptions,
    pub theme: Option<String>,
    pub excluded_extensions: Vec<String>,
}

/// Everything that can go wrong computing a merge view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeMergeDiffError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    req: ComputeMergeDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl GitClient,
) -> Result<ComputeMergeDiffOk, ComputeMergeDiffError> {
    let settings = app_settings.load()?;
    let ComputeMergeDiff { cwd, base, pinned } = req;
    let built = view::build(
        source,
        &cwd,
        base.as_deref(),
        pinned.as_ref(),
        settings.diff_exclusions(),
    )?;
    let excluded_extensions = settings
        .diff_exclusions()
        .for_project_or_default(&built.view.repo_name)
        .extensions()
        .to_vec();

    Ok(ComputeMergeDiffOk {
        view: built.view,
        top: built.top,
        base: built.base,
        diff_range: built.diff_range,
        render_options: settings.viewer_render_options(),
        theme: settings.theme().map(|theme| theme.to_string()),
        excluded_extensions,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{diffs::DiffExclusions, settings::UserSettings, viewer::RenderOptions};

    use super::*;
    use crate::testing::{
        FakeGitClient, FixedUserSettingsStore,
        diffs::{DIFF_SINGLE_FILE, commit},
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
    fn compute_result_retains_the_render_inputs() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let request = ComputeMergeDiff {
            cwd: PathBuf::from("/repo"),
            base: None,
            pinned: None,
        };

        let response = execute(request, &FixedUserSettingsStore::default(), &source)
            .expect("compute succeeds");

        assert_eq!(response.view.repo_root, "/repo");
        assert_eq!(response.top, "/repo");
        assert_eq!(response.base, "main");
        assert_eq!(response.diff_range, "main...HEAD");
        assert_eq!(response.render_options, RenderOptions::DEFAULT);
        assert_eq!(response.theme, None);
        assert!(response.excluded_extensions.is_empty());
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
        let app_settings = FixedUserSettingsStore::new(UserSettings::new(
            None,
            RenderOptions::DEFAULT,
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
        assert_eq!(response.excluded_extensions, ["md"]);
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
                pinned: Some(crate::testing::pinned_range(
                    "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd",
                    "1111111111222222222233333333334444444444",
                )),
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

        let ComputeMergeDiffError::Unexpected(err) = error else {
            panic!("expected Git computation error");
        };
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
