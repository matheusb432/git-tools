//! Builds multi-repository comparisons and reports unavailable local bases.

use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    timestamps::MachineTimestamp,
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{DiffTarget, View, diff_computation},
    ports::{ExtensionFilterReader, GitClient},
    shared::notes::Note,
};

/// One repo to include in a multi-repo render: its resolved top-level path and the
/// label to show on its tab (relative path under the scan root, or the project title).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    pub top: RepositoryRoot,
    pub label: ProjectName,
}

/// The views collected from a batch render, plus how many were skipped.
pub(crate) struct BatchBuild {
    pub views: Vec<View>,
    pub skipped: usize,
}

/// Recursive batches omit empty views; project batches retain them. Default comparisons
/// report unavailable local bases and propagate unexpected failures. Explicit recursive
/// targets retain their existing skip behavior.
pub(crate) fn render_batch(
    git: &impl GitClient,
    target: &DiffTarget,
    filters: &impl ExtensionFilterReader,
    repos: &[RepoRef],
    skip_empty: bool,
    notes: &mut Vec<Note>,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> anyhow::Result<BatchBuild> {
    let mut views = Vec::with_capacity(repos.len());
    let mut skipped = 0usize;
    for repo in repos {
        let filter = filters.extension_filter(&repo.top)?;
        let built = diff_computation::build(git, &repo.top, target, &filter, comparisons);
        if let Err(error) = &built
            && error.is_unavailable()
        {
            notes.push(Note::warn(format!("{}: {error}", repo.label)));
            skipped += 1;
            continue;
        }
        if !skip_empty {
            let mut response = built?;
            notes.append(&mut response.notes);
            let mut view = response.view;
            view.repo_name.clone_from(&repo.label);
            views.push(view);
            continue;
        }

        match built {
            Ok(mut response) if response.view.has_diff_content() => {
                notes.append(&mut response.notes);
                let mut view = response.view;
                view.repo_name.clone_from(&repo.label);
                views.push(view);
            }
            Ok(mut response) => {
                notes.append(&mut response.notes);
                skipped += 1;
            }
            Err(error) if matches!(target, DiffTarget::Unpushed { pinned: None }) => {
                return Err(anyhow::Error::from(error).context(format!("compare {}", repo.label)));
            }
            Err(_) => skipped += 1,
        }
    }
    Ok(BatchBuild { views, skipped })
}

/// `{YYYY-MM-DD} {label}`, dated from `clock`.
pub(crate) fn dated_title(timestamp: &MachineTimestamp, label: &str) -> String {
    let date = timestamp.date();
    format!("{date} {label}")
}

#[cfg(test)]
mod tests {
    use super::{RepoRef, dated_title, render_batch};
    use crate::{
        diffs::DiffTarget,
        utils::{
            FakeGitClient, RepoOverride, SavedExtensionFilters,
            diffs::{DIFF_SINGLE_FILE, commit},
            hiding_extensions, project_name, repository_root,
        },
    };

    fn two_repos() -> Vec<RepoRef> {
        vec![
            RepoRef {
                top: repository_root("/repo-a"),
                label: project_name("repo-a"),
            },
            RepoRef {
                top: repository_root("/repo-b"),
                label: project_name("repo-b"),
            },
        ]
    }

    #[test]
    fn dated_title_prefixes_label_with_yyyy_mm_dd() {
        let timestamp =
            gtl_models::timestamps::MachineTimestamp::try_from("2026-07-02T00:00:00Z").unwrap();

        let title = dated_title(&timestamp, "diff-artifact subrepos");

        assert_eq!(title, "2026-07-02 diff-artifact subrepos");
    }

    #[test]
    fn skip_empty_true_skips_the_empty_view_and_keeps_the_non_empty_one() {
        let mut source = FakeGitClient {
            upstream: Some("origin/main".into()),
            ..Default::default()
        };
        // Default fields (no commits, empty diff) apply to repo-a; repo-b overrides
        // with a real commit + diff, since the fake is otherwise single-scripted.
        source.per_repo.insert(
            "/repo-b".into(),
            RepoOverride {
                commits: vec![commit("abc1234")],
                diff_output: DIFF_SINGLE_FILE.into(),
            },
        );
        let repos = two_repos();
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &SavedExtensionFilters::default(),
            &repos,
            true,
            &mut notes,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(batch.views.len(), 1);
        assert_eq!(batch.skipped, 1);
        assert_eq!(batch.views[0].repo_name.as_str(), "repo-b");
    }

    #[test]
    fn skip_empty_true_counts_a_build_error_as_a_skip() {
        let source = FakeGitClient {
            upstream: None,
            known_revs: vec![], // no "main" fallback either
            ..Default::default()
        };
        let repos = vec![RepoRef {
            top: repository_root("/repo"),
            label: project_name("repo"),
        }];
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &SavedExtensionFilters::default(),
            &repos,
            true,
            &mut notes,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(batch.views.len(), 0);
        assert_eq!(batch.skipped, 1);
    }

    #[test]
    fn skip_empty_false_keeps_an_empty_view() {
        let source = FakeGitClient {
            upstream: Some("origin/main".into()),
            ..Default::default()
        };
        let repos = two_repos();
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &SavedExtensionFilters::default(),
            &repos,
            false,
            &mut notes,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(batch.views.len(), repos.len());
        assert_eq!(batch.skipped, 0);
        assert!(batch.views.iter().all(|view| !view.has_diff_content()));
    }

    #[test]
    fn saved_filters_apply_per_repository_root() {
        const TWO_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n\
diff --git a/notes.md b/notes.md\n\
--- a/notes.md\n\
+++ b/notes.md\n\
@@ -1 +1 @@\n\
-plan\n\
+more plan\n";
        let mut source = FakeGitClient {
            upstream: Some("origin/main".into()),
            ..Default::default()
        };
        for top in ["/repo-a", "/repo-b"] {
            source.per_repo.insert(
                top.into(),
                RepoOverride {
                    commits: vec![commit("abc1234")],
                    diff_output: TWO_FILE_DIFF.into(),
                },
            );
        }
        let filters =
            SavedExtensionFilters::new([(repository_root("/repo-a"), hiding_extensions(&["md"]))]);
        let repos = two_repos();
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &filters,
            &repos,
            true,
            &mut notes,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(batch.views.len(), 2);
        assert_eq!(batch.views[0].files.len(), 1, "repo-a hides notes.md");
        assert!(batch.views[0].extension_filter.is_some());
        assert_eq!(batch.views[1].files.len(), 2, "repo-b is untouched");
        assert!(batch.views[1].extension_filter.is_none());
    }

    #[test]
    fn project_batch_skips_an_unavailable_comparison_with_a_named_warning() {
        let source = FakeGitClient {
            upstream: None,
            known_revs: vec![],
            ..Default::default()
        };
        let repos = two_repos();
        let mut notes = Vec::new();

        let result = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &SavedExtensionFilters::default(),
            &repos,
            false,
            &mut notes,
            &crate::utils::ProjectComparisons::default(),
        );

        assert!(result.unwrap().views.is_empty());
        assert_eq!(notes.len(), 2);
        assert!(notes[0].text.contains("repo"));
    }

    #[test]
    fn recursive_default_comparison_preserves_unexpected_git_failures() {
        let source = crate::utils::ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git process unavailable"
        ))]);
        let error = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &SavedExtensionFilters::default(),
            &two_repos(),
            true,
            &mut Vec::new(),
            &crate::utils::ProjectComparisons::default(),
        )
        .err()
        .unwrap();

        assert_eq!(error.to_string(), "compare repo-a");
        assert_eq!(error.root_cause().to_string(), "git process unavailable");
    }
}
