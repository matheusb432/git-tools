use gtl_models::{failure::ErrorMeta, paths::RepositoryRoot, timestamps::MachineTimestamp};

use crate::{
    diffs::{DiffTarget, View, diff_computation, fetch_full_context_diff},
    ports::{ExtensionFilterReader, GitClient, UserSettingsLoadError, UserSettingsReader},
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeDiff {
    pub repo_root: RepositoryRoot,
    pub target: DiffTarget,
    /// Narrows the diff to changes committed after this time.
    pub changes_since: Option<MachineTimestamp>,
}

#[derive(Debug, Clone)]
pub struct ComputeDiffOk {
    pub view: View,
    pub summary: String,
    pub notes: Vec<Note>,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ComputeDiffError {
    #[error(transparent)]
    #[meta(transparent)]
    Comparison(#[from] crate::projects::comparison::ComparisonError),
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    req: ComputeDiff,
    app_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    filters: &impl ExtensionFilterReader,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<ComputeDiffOk, ComputeDiffError> {
    let ComputeDiff {
        repo_root,
        target,
        changes_since,
    } = req;
    let settings = app_settings.load()?;
    let filter = filters.extension_filter(&repo_root)?;
    let built = diff_computation::build(
        git,
        &repo_root,
        &target,
        &filter,
        comparisons,
        changes_since.as_ref(),
    )?;
    let view = fetch_full_context_diff::load_for_density(
        built.view,
        settings.viewer_render_options().density(),
        git,
    )?;
    Ok(ComputeDiffOk {
        view,
        summary: built.summary,
        notes: built.notes,
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::{ExtensionFilterMode, FileExtensions},
        viewer::DiffDensity,
    };

    use super::*;
    use crate::{
        diffs::{DiffTarget, PinnedRange, compute_diff},
        utils::{
            FakeGitClient, FixedUserSettingsStore, SavedExtensionFilters,
            diffs::{DIFF_SINGLE_FILE, commit},
            hiding_extensions, repository_root, settings_with_density,
        },
    };

    fn req(target: DiffTarget) -> ComputeDiff {
        ComputeDiff {
            repo_root: repository_root("//fixture.invalid/repositories/repo"),
            target,
            changes_since: None,
        }
    }

    fn execute_default_settings(
        request: ComputeDiff,
        source: &FakeGitClient,
    ) -> Result<ComputeDiffOk, ComputeDiffError> {
        compute_diff::execute(
            request,
            &FixedUserSettingsStore::default(),
            source,
            &SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        )
    }

    fn pin() -> PinnedRange {
        crate::utils::pinned_range(
            "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd",
            "1111111111222222222233333333334444444444",
        )
    }

    #[test]
    fn computes_the_view_without_touching_store_or_renderer() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source).unwrap();

        let repository = crate::utils::diffs::repository_origin(&response.view);
        assert_eq!(repository.name.as_str(), "repo");
        assert_eq!(repository.branch.to_string(), "feature");
        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.files[0].path.to_string_lossy(), "f.txt");
        assert_eq!(response.summary, "1 unpushed commit(s)");
        assert_eq!(response.notes, Vec::<crate::shared::notes::Note>::new());
    }

    #[test]
    fn changes_since_starts_the_diff_at_the_last_commit_before_the_cutoff() {
        let at = |id: &str, subject: &str, parent: &str, time: &str| gtl_models::diffs::Commit {
            committed_at: gtl_models::timestamps::MachineTimestamp::try_from(time).unwrap(),
            ..crate::utils::diffs::commit_with(id, subject, &[parent])
        };
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![
                at("cccc", "newer", "bbbb", "2026-09-28T10:00:00Z"),
                at("bbbb", "older", "aaaa", "2026-09-27T10:00:00Z"),
            ],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            ComputeDiff {
                changes_since: Some(
                    gtl_models::timestamps::MachineTimestamp::try_from("2026-09-28T00:00:00Z")
                        .unwrap(),
                ),
                ..req(DiffTarget::Unpushed { pinned: None })
            },
            &source,
        )
        .unwrap();

        let revision =
            |id| gtl_models::git::GitRevision::from(&crate::utils::commit_id_fixture(id));
        assert_eq!(
            response.view.file_filter.source,
            Some(gtl_models::git::GitDiffSpec::Range(
                gtl_models::git::GitRange::two_dot(&revision("bbbb"), &revision("cccc"))
            ))
        );
        assert_eq!(
            response
                .view
                .commits
                .iter()
                .map(|commit| commit.subject.as_str())
                .collect::<Vec<_>>(),
            ["newer"]
        );
    }

    #[test]
    fn compact_compute_defers_full_context_source() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            full_diff_output: format!(
                "{DIFF_SINGLE_FILE} context retained only for full density\n"
            ),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source).unwrap();

        assert!(response.view.files[0].full_lines.is_none());
    }

    #[test]
    fn empty_range_returns_an_empty_view_not_an_error() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source).unwrap();

        assert!(!response.view.has_diff_content());
    }

    #[test]
    fn no_upstream_compares_committed_branch_changes_against_local_main() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: None,
            known_revs: vec!["refs/heads/main".into(), "HEAD".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source).unwrap();

        assert_eq!(
            response.notes,
            vec![Note::info(
                "diff-artifact: no upstream; comparing branch changes against main"
            )]
        );
    }

    #[test]
    fn pinned_unpushed_computes_without_an_upstream() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: None,
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            req(DiffTarget::Unpushed {
                pinned: Some(pin()),
            }),
            &source,
        )
        .unwrap();

        assert!(
            response.notes.is_empty(),
            "no fallback note for a pinned range"
        );
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.view.foot.cmd, "git diff aaaaaaaaaa..1111111111");
        assert_eq!(
            crate::utils::diffs::repository_origin(&response.view)
                .upstream
                .as_ref(),
            "aaaaaaaaaa"
        );
        assert_eq!(response.summary, "1 unpushed commit(s)");
    }

    #[test]
    fn pinned_merge_target_skips_symbolic_verification() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            req(DiffTarget::Merge {
                base: crate::utils::git_revision("main"),
                pinned: Some(pin()),
            }),
            &source,
        )
        .unwrap();

        assert_eq!(
            response.view.title,
            gtl_models::diffs::DiffViewTitle::MergeDiff
        );
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.summary, "to merge into main");
    }

    #[test]
    fn pinned_range_target_computes_over_the_pin_with_exact_range_labels() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            req(DiffTarget::Range {
                range: crate::utils::git_range("a..b"),
                pinned: Some(pin()),
            }),
            &source,
        )
        .unwrap();

        assert_eq!(response.notes, Vec::<crate::shared::notes::Note>::new());
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };

        let error = execute_default_settings(
            req(DiffTarget::Base(crate::utils::git_revision("nope"))),
            &source,
        )
        .unwrap_err();

        let err = match error {
            ComputeDiffError::Unexpected(error)
            | ComputeDiffError::Comparison(
                crate::projects::comparison::ComparisonError::Unexpected(error),
            ) => Some(error),
            ComputeDiffError::Settings(_) | ComputeDiffError::Comparison(_) => None,
        }
        .unwrap();
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }

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

    fn saved(
        repository: &str,
        filter: gtl_models::diffs::ExtensionFilter,
    ) -> SavedExtensionFilters {
        SavedExtensionFilters::new([(repository_root(repository), filter)])
    }

    #[test]
    fn full_compute_loads_the_deferred_source() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            full_diff_output: format!("{DIFF_SINGLE_FILE} retained context\n"),
            ..Default::default()
        };

        let response = compute_diff::execute(
            req(DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::new(settings_with_density(DiffDensity::Full)),
            &source,
            &SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(
            response.view.files[0]
                .full_lines
                .as_ref()
                .is_some_and(|lines| lines.iter().any(|line| line == " retained context"))
        );
        assert_eq!(
            response.view.full_context,
            crate::diffs::FullContextDiffState::Loaded
        );
    }

    #[test]
    fn full_compute_does_not_fetch_when_no_modified_file_needs_context() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: "diff --git a/new.txt b/new.txt\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/new.txt\n\
@@ -0,0 +1 @@\n\
+new\n"
                .into(),
            full_diff_output: "diff --git a/../invalid b/../invalid\n".into(),
            ..Default::default()
        };

        let response = compute_diff::execute(
            req(DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::new(settings_with_density(DiffDensity::Full)),
            &source,
            &SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.files[0].path.to_string_lossy(), "new.txt");
        assert!(matches!(
            response.view.full_context,
            crate::diffs::FullContextDiffState::Loaded
        ));
    }

    #[test]
    fn saved_hidden_extensions_are_hidden_and_reported() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let request = req(DiffTarget::Unpushed { pinned: None });

        let response = compute_diff::execute(
            request,
            &FixedUserSettingsStore::default(),
            &source,
            &saved(
                "//fixture.invalid/repositories/repo",
                hiding_extensions(&["md"]),
            ),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        let paths: Vec<String> = response
            .view
            .files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(paths, ["f.txt"], "the .md file is hidden");
        let applied = response.view.extension_filter.unwrap();
        assert_eq!(applied.filter, hiding_extensions(&["md"]));
        assert_eq!(
            applied
                .hidden_paths
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>(),
            ["docs/notes.md"]
        );
        assert!(
            response.notes.contains(&Note::info(
                "diff-artifact: 1 file(s) hidden by the saved extension filter (hide md)"
            )),
            "filter note missing: {:?}",
            response.notes
        );
    }

    #[test]
    fn show_only_filters_hide_every_unlisted_extension() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let only_markdown = gtl_models::diffs::ExtensionFilter::new(
            ExtensionFilterMode::Only,
            FileExtensions::new(["md"]),
        );

        let response = compute_diff::execute(
            req(DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &saved("//fixture.invalid/repositories/repo", only_markdown.clone()),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(
            response
                .view
                .files
                .iter()
                .map(|file| file.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["docs/notes.md"]
        );
        let applied = response.view.extension_filter.unwrap();
        assert_eq!(applied.filter, only_markdown);
        assert_eq!(
            applied.hidden_paths,
            [crate::utils::repository_relative_path("f.txt")]
        );
    }

    #[test]
    fn filters_saved_for_another_repository_do_not_apply() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let request = req(DiffTarget::Unpushed { pinned: None });

        let response = compute_diff::execute(
            request,
            &FixedUserSettingsStore::default(),
            &source,
            &saved(
                "//fixture.invalid/repositories/other/repo",
                hiding_extensions(&["md"]),
            ),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.view.files.len(), 2);
        assert_eq!(response.view.extension_filter, None);
        assert_eq!(response.notes, Vec::<crate::shared::notes::Note>::new());
    }

    #[test]
    fn idle_filters_matching_nothing_stay_invisible() {
        let source = FakeGitClient {
            top_level: Some("//fixture.invalid/repositories/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let request = req(DiffTarget::Unpushed { pinned: None });

        let response = compute_diff::execute(
            request,
            &FixedUserSettingsStore::default(),
            &source,
            &saved(
                "//fixture.invalid/repositories/repo",
                hiding_extensions(&["md"]),
            ),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.extension_filter, None);
        assert_eq!(response.notes, Vec::<crate::shared::notes::Note>::new());
    }
}
