//! The `compute_diff` vertical slice: resolve a [`DiffTarget`] into the
//! structured [`View`] the native viewer renders — no HTML, no artifact store.
//! The desktop's in-process mediator dispatches this for recipe tabs; the
//! daemon's `--raw` path keeps using `render_diff`.

use std::path::PathBuf;

use domain::diffs::{DiffExclusions, DiffTarget, View};

use crate::{diffs::render_diff::build_view, ports::DiffSource, shared::notes::Note};

/// Compute the structured diff view for `target`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeDiff {
    pub cwd: PathBuf,
    pub target: DiffTarget,
    pub exclusions: DiffExclusions,
}

/// The computed view plus the human summary and every surfaced message.
/// An empty view is a legitimate outcome (`view.is_empty()`); tab policy is the
/// caller's concern.
#[derive(Debug, Clone)]
pub struct ComputeDiffResponse {
    pub view: View,
    pub summary: String,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong computing a diff view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeDiffError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Computes a diff by driving the diff engine through its source port.
#[cqrsy::handler(query)]
pub fn execute(
    req: ComputeDiff,
    source: &impl DiffSource,
) -> Result<ComputeDiffResponse, ComputeDiffError> {
    let ComputeDiff {
        cwd,
        target,
        exclusions,
    } = req;
    let mut notes = Vec::new();
    let top = source.top_level(&cwd)?;
    let (view, summary) = build_view(source, &top, &target, None, &exclusions, &mut notes)?;
    Ok(ComputeDiffResponse {
        view,
        summary,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::{Commit, DiffTarget};

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

    fn one_commit() -> Commit {
        Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }
    }

    fn req(target: DiffTarget) -> ComputeDiff {
        ComputeDiff {
            cwd: PathBuf::from("/repo"),
            target,
            exclusions: DiffExclusions::default(),
        }
    }

    fn pin() -> domain::diffs::PinnedRange {
        domain::diffs::PinnedRange {
            base: "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd".into(),
            head: "1111111111222222222233333333334444444444".into(),
        }
    }

    #[test]
    fn computes_the_view_without_touching_store_or_renderer() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response =
            execute(req(DiffTarget::Unpushed { pinned: None }), &source).expect("compute succeeds");

        assert_eq!(response.view.repo_name, "repo");
        assert_eq!(response.view.branch, "feature");
        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.files[0].path, "f.txt");
        assert_eq!(response.summary, "1 unpushed commit(s)");
        assert!(response.notes.is_empty());
    }

    #[test]
    fn empty_range_returns_an_empty_view_not_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            ..Default::default()
        };

        let response =
            execute(req(DiffTarget::Unpushed { pinned: None }), &source).expect("compute succeeds");

        assert!(response.view.is_empty());
    }

    #[test]
    fn no_upstream_falls_back_to_main_with_the_warning_note() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            known_revs: vec!["main".into()],
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response =
            execute(req(DiffTarget::Unpushed { pinned: None }), &source).expect("compute succeeds");

        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff-preview: no upstream; falling back to main"
            )]
        );
    }

    #[test]
    fn pinned_unpushed_computes_without_an_upstream_and_without_the_fallback_note() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None, // would warn-and-fallback (or error) symbolically
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response = execute(
            req(DiffTarget::Unpushed {
                pinned: Some(pin()),
            }),
            &source,
        )
        .expect("pinned compute succeeds");

        assert!(
            response.notes.is_empty(),
            "no fallback note for a pinned range"
        );
        assert_eq!(response.view.commits_label, "# unpushed commits");
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.view.foot.cmd, "git diff aaaaaaaaaa..1111111111");
        assert_eq!(response.view.upstream, "aaaaaaaaaa");
        assert_eq!(response.summary, "1 unpushed commit(s)");
    }

    #[test]
    fn pinned_merge_target_skips_symbolic_verification() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![], // symbolic verify_commit("main") would error
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response = execute(
            req(DiffTarget::Merge {
                base: "main".into(),
                pinned: Some(pin()),
            }),
            &source,
        )
        .expect("pinned merge computes");

        assert_eq!(response.view.title, "merge-diff");
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.summary, "to merge into main");
    }

    #[test]
    fn pinned_range_target_computes_over_the_pin_with_exact_range_labels() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![], // symbolic verify_exact_range would error
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };

        let response = execute(
            req(DiffTarget::Range {
                range: "a..b".into(),
                pinned: Some(pin()),
            }),
            &source,
        )
        .expect("pinned range computes");

        assert!(response.notes.is_empty());
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.view.commits_label, "# commits in range");
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };

        let error = execute(req(DiffTarget::Base("nope".into())), &source)
            .expect_err("unknown base errors");

        let ComputeDiffError::Unexpected(err) = error;
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

    fn excluding(project: &str, extensions: &[&str]) -> DiffExclusions {
        DiffExclusions::new(
            [(
                project.to_string(),
                extensions.iter().map(ToString::to_string).collect(),
            )],
            None,
        )
    }

    #[test]
    fn configured_extensions_are_hidden_and_reported() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let mut request = req(DiffTarget::Unpushed { pinned: None });
        request.exclusions = excluding("repo", &["md"]);

        let response = execute(request, &source).expect("compute succeeds");

        let paths: Vec<&str> = response
            .view
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect();
        assert_eq!(paths, ["f.txt"], "the .md file is hidden");
        let applied = response
            .view
            .exclusions
            .expect("hidden files carry a summary");
        assert_eq!(applied.extensions, ["md"]);
        assert_eq!(applied.hidden_paths, ["docs/notes.md"]);
        assert!(
            response.notes.contains(&Note::info(
                "diff-preview: 1 file(s) hidden by config [diff.exclude] (md)"
            )),
            "exclusion note missing: {:?}",
            response.notes
        );
    }

    #[test]
    fn exclusions_for_another_project_do_not_apply() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let mut request = req(DiffTarget::Unpushed { pinned: None });
        request.exclusions = excluding("other-repo", &["md"]);

        let response = execute(request, &source).expect("compute succeeds");

        assert_eq!(response.view.files.len(), 2);
        assert_eq!(response.view.exclusions, None);
        assert!(response.notes.is_empty());
    }

    #[test]
    fn idle_exclusions_matching_nothing_stay_invisible() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };
        let mut request = req(DiffTarget::Unpushed { pinned: None });
        request.exclusions = excluding("repo", &["md"]);

        let response = execute(request, &source).expect("compute succeeds");

        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.exclusions, None);
        assert!(response.notes.is_empty());
    }
}
