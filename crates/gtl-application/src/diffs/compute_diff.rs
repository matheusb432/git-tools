//! The `compute_diff` vertical slice: resolve a [`DiffTarget`] into the
//! structured [`View`] the native viewer renders — no HTML, no artifact store.
//! The desktop's in-process mediator dispatches this for recipe tabs; the
//! daemon's `--raw` path keeps using `render_diff`.

use std::path::PathBuf;

use crate::{
    diffs::{DiffTarget, View, diff_computation},
    ports::{GitClient, UserSettingsLoadError, UserSettingsStore},
    shared::notes::Note,
};

/// Compute the structured diff view for `target`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeDiff {
    pub cwd: PathBuf,
    pub target: DiffTarget,
}

/// The computed view plus the human summary and every surfaced message.
/// A view without diff content is a legitimate outcome; tab policy is the caller's concern.
#[derive(Debug, Clone)]
pub struct ComputeDiffOk {
    pub view: View,
    pub summary: String,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong computing a diff view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeDiffError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Computes a diff by driving the diff engine through its source port.
#[cqrsy::query]
pub fn execute(
    req: ComputeDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl GitClient,
) -> Result<ComputeDiffOk, ComputeDiffError> {
    let ComputeDiff { cwd, target } = req;
    let settings = app_settings.load()?;
    let top = source.top_level(&cwd)?;
    let built = diff_computation::build(source, &top, &target, settings.diff_exclusions())?;
    Ok(ComputeDiffOk {
        view: built.view,
        summary: built.summary,
        notes: built.notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{diffs::DiffExclusions, settings::UserSettings, viewer::RenderOptions};

    use super::*;
    use crate::{
        diffs::{
            DiffTarget, PinnedRange,
            logic::range_view::{
                LABEL_COMMITS_IN_RANGE, LABEL_UNPUSHED_COMMITS, NOTE_WORKING_TREE,
            },
        },
        testing::{
            FakeGitClient, FixedUserSettingsStore,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(target: DiffTarget) -> ComputeDiff {
        ComputeDiff {
            cwd: PathBuf::from("/repo"),
            target,
        }
    }

    fn execute_default_settings(
        request: ComputeDiff,
        source: &FakeGitClient,
    ) -> Result<ComputeDiffOk, ComputeDiffError> {
        execute(request, &FixedUserSettingsStore::default(), source)
    }

    fn pin() -> PinnedRange {
        PinnedRange {
            base: "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd".into(),
            head: "1111111111222222222233333333334444444444".into(),
        }
    }

    #[test]
    fn computes_the_view_without_touching_store_or_renderer() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source)
                .expect("compute succeeds");

        assert_eq!(response.view.repo_name, "repo");
        assert_eq!(response.view.branch, "feature");
        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.files[0].path, "f.txt");
        assert_eq!(response.view.commits_label, LABEL_UNPUSHED_COMMITS);
        assert_eq!(response.summary, "1 unpushed commit(s)");
        assert!(response.notes.is_empty());
    }

    #[test]
    fn empty_range_returns_an_empty_view_not_an_error() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source)
                .expect("compute succeeds");

        assert!(!response.view.has_diff_content());
    }

    #[test]
    fn no_upstream_falls_back_to_main_with_the_warning_note() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response =
            execute_default_settings(req(DiffTarget::Unpushed { pinned: None }), &source)
                .expect("compute succeeds");

        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff-artifact: no upstream; falling back to main"
            )]
        );
        assert_eq!(response.view.foot.note, NOTE_WORKING_TREE);
    }

    #[test]
    fn pinned_unpushed_computes_without_an_upstream() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None, // would warn-and-fallback (or error) symbolically
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
        .expect("pinned compute succeeds");

        assert!(
            response.notes.is_empty(),
            "no fallback note for a pinned range"
        );
        assert_eq!(response.view.commits_label, LABEL_COMMITS_IN_RANGE);
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.view.foot.cmd, "git diff aaaaaaaaaa..1111111111");
        assert_eq!(response.view.upstream, "aaaaaaaaaa");
        assert_eq!(response.summary, "1 unpushed commit(s)");
    }

    #[test]
    fn pinned_merge_target_skips_symbolic_verification() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![], // symbolic verify_commit("main") would error
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
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
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![], // symbolic verify_exact_range would error
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            req(DiffTarget::Range {
                range: "a..b".into(),
                pinned: Some(pin()),
            }),
            &source,
        )
        .expect("pinned range computes");

        assert!(response.notes.is_empty());
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(response.view.commits_label, LABEL_COMMITS_IN_RANGE);
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };

        let error = execute_default_settings(req(DiffTarget::Base("nope".into())), &source)
            .expect_err("unknown base errors");

        let ComputeDiffError::Unexpected(err) = error else {
            panic!("expected Git computation error");
        };
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

    fn settings(exclusions: DiffExclusions) -> UserSettings {
        UserSettings::new(None, RenderOptions::DEFAULT, true, exclusions)
    }

    #[test]
    fn configured_extensions_are_hidden_and_reported() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let request = req(DiffTarget::Unpushed { pinned: None });
        let app_settings = FixedUserSettingsStore::new(settings(excluding("repo", &["md"])));

        let response = execute(request, &app_settings, &source).expect("compute succeeds");

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
                "diff-artifact: 1 file(s) hidden by config [diff.exclude] (md)"
            )),
            "exclusion note missing: {:?}",
            response.notes
        );
    }

    #[test]
    fn exclusions_for_another_project_do_not_apply() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: CODE_AND_NOTES_DIFF.into(),
            ..Default::default()
        };
        let request = req(DiffTarget::Unpushed { pinned: None });
        let app_settings = FixedUserSettingsStore::new(settings(excluding("other-repo", &["md"])));

        let response = execute(request, &app_settings, &source).expect("compute succeeds");

        assert_eq!(response.view.files.len(), 2);
        assert_eq!(response.view.exclusions, None);
        assert!(response.notes.is_empty());
    }

    #[test]
    fn idle_exclusions_matching_nothing_stay_invisible() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let request = req(DiffTarget::Unpushed { pinned: None });
        let app_settings = FixedUserSettingsStore::new(settings(excluding("repo", &["md"])));

        let response = execute(request, &app_settings, &source).expect("compute succeeds");

        assert_eq!(response.view.files.len(), 1);
        assert_eq!(response.view.exclusions, None);
        assert!(response.notes.is_empty());
    }
}
