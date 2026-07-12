//! Shared multi-repo render helper for the diff-subrepos and diff-all slices:
//! builds one [`View`] per repo via [`build_view`], with `skip_empty` controlling
//! whether an empty or errored view is skipped-and-counted (diff-subrepos) or kept
//! and propagated (diff-all).

use domain::diffs::{DiffTarget, View};

use crate::{
    diffs::render_diff::build_view,
    ports::{Clock, DiffSource},
    shared::notes::Note,
};

/// One repo to include in a multi-repo render: its resolved top-level path and the
/// label to show on its tab (relative path under the scan root, or the manifest name).
#[derive(Debug, Clone, PartialEq)]
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
    theme: Option<&str>,
    repos: &[RepoRef],
    skip_empty: bool,
    notes: &mut Vec<Note>,
) -> anyhow::Result<BatchBuild> {
    let mut views = Vec::with_capacity(repos.len());
    let mut skipped = 0usize;
    for repo in repos {
        let built = build_view(source, &repo.top, target, theme.map(String::from), notes);
        if skip_empty {
            match built {
                Ok((mut view, _)) if !view.is_empty() => {
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
    use domain::diffs::{Commit, DiffTarget};

    use super::{RepoRef, dated_title, render_batch};
    use crate::testing::{FakeDiffSource, FixedClock, RepoOverride};

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
        index 111..222 100644\n\
        --- a/f.txt\n\
        +++ b/f.txt\n\
        @@ -1,2 +1,3 @@\n\
         keep\n\
        -old line\n\
        +new line\n\
        +extra line\n";

    fn one_commit() -> Commit {
        Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }
    }

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
                commits: vec![one_commit()],
                diff_output: SINGLE_FILE_DIFF.into(),
            },
        );
        let repos = two_repos();
        let mut notes = Vec::new();

        let batch = render_batch(
            &source,
            &DiffTarget::Unpushed { pinned: None },
            None,
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
            None,
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
            None,
            &repos,
            false,
            &mut notes,
        )
        .expect("batch succeeds");

        assert_eq!(batch.views.len(), repos.len());
        assert_eq!(batch.skipped, 0);
        assert!(batch.views.iter().all(domain::diffs::View::is_empty));
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
            None,
            &repos,
            false,
            &mut notes,
        );

        assert!(result.is_err());
    }
}
