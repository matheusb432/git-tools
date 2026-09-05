//! Shared multi-repo render helper for the diff-subrepos and diff-all slices:
//! builds one [`View`] per repo through the normal diff compute core, with `skip_empty` controlling
//! whether an empty or errored view is skipped-and-counted (diff-subrepos) or kept
//! and propagated (diff-all).

use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    settings::UserSettings,
    timestamps::MachineTimestamp,
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{DiffTarget, View, diff_computation},
    ports::GitClient,
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

/// Builds one [`View`] per repo through the normal diff compute core, accumulating every note into
/// `notes` in repo order. `skip_empty`: when true, an error or an empty view is
/// counted as a skip (diff-subrepos); when false, a build error propagates and every
/// view is kept regardless of emptiness (diff --all -- matches its current no-skip
/// behavior exactly, do not "fix" this asymmetry).
pub(crate) fn render_batch(
    git: &impl GitClient,
    target: &DiffTarget,
    settings: &UserSettings,
    repos: &[RepoRef],
    skip_empty: bool,
    notes: &mut Vec<Note>,
) -> anyhow::Result<BatchBuild> {
    let mut views = Vec::with_capacity(repos.len());
    let mut skipped = 0usize;
    for repo in repos {
        let built = diff_computation::build(git, &repo.top, target, settings.diff_exclusions());
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
    use gtl_models::{diffs::DiffExclusions, settings::UserSettings, viewer::RenderOptions};

    use super::{RepoRef, dated_title, render_batch};
    use crate::{
        diffs::DiffTarget,
        utils::{
            FakeGitClient, RepoOverride, default_user_settings,
            diffs::{DIFF_SINGLE_FILE, commit},
            project_name, repository_root,
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

    fn settings(exclusions: DiffExclusions) -> UserSettings {
        UserSettings::new(
            None,
            RenderOptions::DEFAULT,
            gtl_models::viewer::ViewerKeybindings::default(),
            true,
            exclusions,
            gtl_models::settings::PushAllExclusions::default(),
        )
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
            &default_user_settings(),
            &repos,
            true,
            &mut notes,
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
            &default_user_settings(),
            &repos,
            true,
            &mut notes,
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
            &default_user_settings(),
            &repos,
            false,
            &mut notes,
        )
        .unwrap();

        assert_eq!(batch.views.len(), repos.len());
        assert_eq!(batch.skipped, 0);
        assert!(batch.views.iter().all(|view| !view.has_diff_content()));
    }

    #[test]
    fn exclusions_apply_per_repo_by_directory_name() {
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
        let exclusions = gtl_models::diffs::DiffExclusions::new(
            [(project_name("repo-a"), vec!["md".to_string()])],
            None,
        );
        let settings = settings(exclusions);
        let repos = two_repos();
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &settings,
            &repos,
            true,
            &mut notes,
        )
        .unwrap();

        assert_eq!(batch.views.len(), 2);
        assert_eq!(batch.views[0].files.len(), 1, "repo-a hides notes.md");
        assert!(batch.views[0].exclusions.is_some());
        assert_eq!(batch.views[1].files.len(), 2, "repo-b is untouched");
        assert!(batch.views[1].exclusions.is_none());
    }

    #[test]
    fn skip_empty_false_propagates_a_build_error() {
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
            &default_user_settings(),
            &repos,
            false,
            &mut notes,
        );

        assert!(result.is_err());
    }
}
