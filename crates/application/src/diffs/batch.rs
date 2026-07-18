//! Shared multi-repo render helper for the diff-subrepos and diff-all slices:
//! builds one [`View`] per repo via [`build_view`], with `skip_empty` controlling
//! whether an empty or errored view is skipped-and-counted (diff-subrepos) or kept
//! and propagated (diff-all).

use serde::{Deserialize, Serialize};

use crate::{
    diffs::{DiffTarget, View, render_diff::build_view},
    ports::{AppSettings, Clock, DiffSource},
    shared::notes::Note,
};

/// One repo to include in a multi-repo render: its resolved top-level path and the
/// label to show on its tab (relative path under the scan root, or the manifest name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    pub top: String,
    pub label: String,
}

/// The views collected from a batch render, plus how many were skipped.
pub(crate) struct BatchBuild {
    pub views: Vec<View>,
    pub skipped: usize,
}

/// Builds one [`View`] per repo via [`build_view`], accumulating every note into
/// `notes` in repo order. `skip_empty`: when true, an error or an empty view is
/// counted as a skip (diff-subrepos); when false, a build error propagates and every
/// view is kept regardless of emptiness (diff --all -- matches its current no-skip
/// behavior exactly, do not "fix" this asymmetry).
pub(crate) fn render_batch(
    source: &impl DiffSource,
    target: &DiffTarget,
    settings: &AppSettings,
    repos: &[RepoRef],
    skip_empty: bool,
    notes: &mut Vec<Note>,
) -> anyhow::Result<BatchBuild> {
    let mut views = Vec::with_capacity(repos.len());
    let mut skipped = 0usize;
    for repo in repos {
        let built = build_view(
            source,
            &repo.top,
            target,
            settings.theme().map(str::to_owned),
            settings.diff_exclusions(),
            notes,
        );
        if skip_empty {
            match built {
                Ok((mut view, _)) if view.has_diff_content() => {
                    view.repo_name.clone_from(&repo.label);
                    views.push(view);
                }
                Ok(_) | Err(_) => skipped += 1,
            }
        } else {
            let (mut view, _) = built?;
            view.repo_name.clone_from(&repo.label);
            views.push(view);
        }
    }
    Ok(BatchBuild { views, skipped })
}

/// `{YYYY-MM-DD} {label}`, dated from `clock` (its `now_iso()` is already ISO-8601, so
/// the first 10 chars are the date -- no new date-formatting dependency needed).
pub(crate) fn dated_title(clock: &impl Clock, label: &str) -> String {
    let date = &clock.now_iso()[..10];
    format!("{date} {label}")
}

#[cfg(test)]
mod tests {
    use domain::diffs::DiffExclusions;

    use super::{RepoRef, dated_title, render_batch};
    use crate::{
        diffs::DiffTarget,
        ports::AppSettings,
        testing::{
            FakeDiffSource, FixedClock, RepoOverride,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn two_repos() -> Vec<RepoRef> {
        vec![
            RepoRef {
                top: "/repo-a".into(),
                label: "repo-a".into(),
            },
            RepoRef {
                top: "/repo-b".into(),
                label: "repo-b".into(),
            },
        ]
    }

    #[test]
    fn dated_title_prefixes_label_with_yyyy_mm_dd() {
        let clock = FixedClock("2026-07-02T00:00:00Z".into());

        let title = dated_title(&clock, "diff-preview subrepos");

        assert_eq!(title, "2026-07-02 diff-preview subrepos");
    }

    #[test]
    fn skip_empty_true_skips_the_empty_view_and_keeps_the_non_empty_one() {
        let mut source = FakeDiffSource {
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
            &AppSettings::default(),
            &repos,
            true,
            &mut notes,
        )
        .expect("batch succeeds");

        assert_eq!(batch.views.len(), 1);
        assert_eq!(batch.skipped, 1);
        assert_eq!(batch.views[0].repo_name, "repo-b");
    }

    #[test]
    fn skip_empty_true_counts_a_build_error_as_a_skip() {
        let source = FakeDiffSource {
            upstream: None,
            known_revs: vec![], // no "main" fallback either
            ..Default::default()
        };
        let repos = vec![RepoRef {
            top: "/repo".into(),
            label: "repo".into(),
        }];
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &AppSettings::default(),
            &repos,
            true,
            &mut notes,
        )
        .expect("a build error is swallowed as a skip, not propagated");

        assert_eq!(batch.views.len(), 0);
        assert_eq!(batch.skipped, 1);
    }

    #[test]
    fn skip_empty_false_keeps_an_empty_view() {
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            ..Default::default()
        };
        let repos = two_repos();
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &AppSettings::default(),
            &repos,
            false,
            &mut notes,
        )
        .expect("batch succeeds");

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
        let mut source = FakeDiffSource {
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
        let exclusions = domain::diffs::DiffExclusions::new(
            [("repo-a".to_string(), vec!["md".to_string()])],
            None,
        );
        let settings = AppSettings::new(None, true, exclusions);
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
        .expect("batch succeeds");

        assert_eq!(batch.views.len(), 2);
        assert_eq!(batch.views[0].files.len(), 1, "repo-a hides notes.md");
        assert!(batch.views[0].exclusions.is_some());
        assert_eq!(batch.views[1].files.len(), 2, "repo-b is untouched");
        assert!(batch.views[1].exclusions.is_none());
    }

    #[test]
    fn settings_value_supplies_theme_and_project_exclusions_to_every_view() {
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
        let mut source = FakeDiffSource {
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
        let settings = AppSettings::new(
            Some("night".into()),
            true,
            DiffExclusions::new([("repo-a".to_string(), vec!["md"])], None),
        );
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &settings,
            &two_repos(),
            true,
            &mut notes,
        )
        .expect("batch succeeds");

        assert!(
            batch
                .views
                .iter()
                .all(|view| view.theme.as_deref() == Some("night"))
        );
        assert_eq!(batch.views[0].files.len(), 1);
        assert_eq!(batch.views[1].files.len(), 2);
    }

    #[test]
    fn skip_empty_false_propagates_a_build_error() {
        let source = FakeDiffSource {
            upstream: None,
            known_revs: vec![],
            ..Default::default()
        };
        let repos = two_repos();
        let mut notes = Vec::new();

        let result = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            &AppSettings::default(),
            &repos,
            false,
            &mut notes,
        );

        assert!(result.is_err());
    }
}
