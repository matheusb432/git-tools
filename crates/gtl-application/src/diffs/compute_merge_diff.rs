mod view;

use gtl_models::{
    diffs::ExcludedExtensions,
    failure::ErrorMeta,
    git::{GitDiffSpec, GitRevision},
    paths::RepositoryRoot,
    viewer::Theme,
};

use crate::{
    diffs::{
        FetchFullContextDiff, FullContextDiffState, PinnedRange, View, fetch_full_context_diff,
    },
    ports::{GitClient, UserSettingsLoadError, UserSettingsReader},
};

pub const DEFAULT_BASE: &str = crate::shared::git_range_pinning::DEFAULT_MERGE_BASE;

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeMergeDiff {
    pub repo_root: RepositoryRoot,
    pub base: Option<GitRevision>,
    pub pinned: Option<PinnedRange>,
}

#[derive(Debug, Clone)]
pub struct ComputeMergeDiffOk {
    pub view: View,
    pub top: RepositoryRoot,
    pub base: GitRevision,
    pub diff_range: GitDiffSpec,
    pub render_options: gtl_models::viewer::RenderOptions,
    pub theme: Option<Theme>,
    pub language: gtl_models::settings::ViewerLanguage,
    pub excluded_extensions: ExcludedExtensions,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ComputeMergeDiffError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    req: ComputeMergeDiff,
    app_settings: &impl UserSettingsReader,
    git: &impl GitClient,
) -> Result<ComputeMergeDiffOk, ComputeMergeDiffError> {
    let settings = app_settings.load()?;
    let ComputeMergeDiff {
        repo_root,
        base,
        pinned,
    } = req;
    let mut built = view::build(
        git,
        &repo_root,
        base.as_ref(),
        pinned.as_ref(),
        settings.diff_exclusions(),
    )?;
    if settings.viewer_render_options().density() == gtl_models::viewer::DiffDensity::Full
        && let FullContextDiffState::Deferred(source) = &built.view.full_context
    {
        let request = FetchFullContextDiff::new(&built.view.repo_root, source);
        let full_context =
            fetch_full_context_diff::execute(&request, git).map_err(anyhow::Error::from)?;
        built.view = built
            .view
            .with_full_context(full_context)
            .map_err(anyhow::Error::from)?;
    }
    let excluded_extensions = settings
        .diff_exclusions()
        .for_project_or_default(&built.view.repo_name)
        .clone();

    Ok(ComputeMergeDiffOk {
        view: built.view,
        top: built.top,
        base: built.base,
        diff_range: built.diff_range,
        render_options: settings.viewer_render_options(),
        theme: settings.theme(),
        language: settings.language(),
        excluded_extensions,
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{diffs::DiffExclusions, settings::UserSettings, viewer::RenderOptions};

    use super::*;
    use crate::{
        diffs::compute_merge_diff,
        utils::{
            FakeGitClient, FixedUserSettingsStore,
            diffs::{DIFF_SINGLE_FILE, commit},
            project_name, repository_root,
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
        compute_merge_diff::execute(request, &FixedUserSettingsStore::default(), source)
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
            repo_root: repository_root("/repo"),
            base: None,
            pinned: None,
        };

        let response =
            compute_merge_diff::execute(request, &FixedUserSettingsStore::default(), &source)
                .unwrap();

        assert_eq!(
            response.view.repo_root.as_ref(),
            std::path::Path::new("/repo")
        );
        assert_eq!(response.top.as_ref(), std::path::Path::new("/repo"));
        assert_eq!(response.base.as_ref(), "main");
        assert_eq!(response.diff_range.as_arg(), "main...HEAD");
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
            gtl_models::viewer::ViewerKeybindings::default(),
            true,
            DiffExclusions::new([(project_name("repo"), vec!["md"])], None),
            gtl_models::settings::PushAllExclusions::default(),
        ));

        let response = compute_merge_diff::execute(
            ComputeMergeDiff {
                repo_root: repository_root("/repo"),
                base: None,
                pinned: None,
            },
            &app_settings,
            &source,
        )
        .unwrap();

        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.excluded_extensions.extensions(), ["md"]);
        assert_eq!(response.view.files[0].path.to_string_lossy(), "f.txt");
        assert_eq!(
            response
                .view
                .exclusions
                .unwrap()
                .hidden_paths
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>(),
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
                repo_root: repository_root("/repo"),
                base: None,
                pinned: None,
            },
            &source,
        )
        .unwrap();

        assert_eq!(response.view.upstream.as_ref(), "main");
        assert_eq!(response.view.branch.to_string(), "feature");
        assert_eq!(response.view.files.len(), 1);
    }

    #[test]
    fn pinned_merge_diff_computes_over_the_pinned_range_without_verification() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            ComputeMergeDiff {
                repo_root: repository_root("/repo"),
                base: None,
                pinned: Some(crate::utils::pinned_range(
                    "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd",
                    "1111111111222222222233333333334444444444",
                )),
            },
            &source,
        )
        .unwrap();

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
                repo_root: repository_root("/repo"),
                base: Some(crate::utils::git_revision("nope")),
                pinned: None,
            },
            &source,
        )
        .unwrap_err();

        let err = match error {
            ComputeMergeDiffError::Unexpected(error) => Some(error),
            ComputeMergeDiffError::Settings(_) => None,
        }
        .unwrap();
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
