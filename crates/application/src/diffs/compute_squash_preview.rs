//! The `compute_squash_preview` vertical slice: the structured squash-preview
//! [`View`] for the native viewer — no HTML, no artifact store.

mod view;

use std::path::PathBuf;

pub(crate) use view::SquashViewBuild;

use crate::{
    diffs::{PinnedRange, View},
    ports::{AppSettings, DiffSource, UserSettingsStore},
};

/// Compute the squash-preview view of the current branch's unpushed commits
/// (base is always the configured upstream), resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeSquashPreview {
    pub cwd: PathBuf,
    pub pinned: Option<PinnedRange>,
}

/// The computed squash-preview view.
#[derive(Debug, Clone)]
pub struct ComputeSquashPreviewResponse {
    pub view: View,
}

/// Everything that can go wrong computing a squash-preview view.
#[derive(Debug, thiserror::Error)]
pub enum ComputeSquashPreviewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Computes a squash preview via the shared squash view builder.
#[cqrsy::query]
pub fn execute(
    req: ComputeSquashPreview,
    app_settings: &impl UserSettingsStore,
    source: &impl DiffSource,
) -> Result<ComputeSquashPreviewResponse, ComputeSquashPreviewError> {
    let settings = app_settings.load();
    let built = compute(req, &settings, source)?;
    Ok(ComputeSquashPreviewResponse { view: built.view })
}

pub(crate) fn compute(
    req: ComputeSquashPreview,
    settings: &AppSettings,
    source: &impl DiffSource,
) -> anyhow::Result<SquashViewBuild> {
    let ComputeSquashPreview { cwd, pinned } = req;
    view::build(source, &cwd, pinned.as_ref(), settings.diff_exclusions())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::DiffExclusions;

    use super::*;
    use crate::{
        ports::AppSettings,
        testing::{
            FakeDiffSource, FixedUserSettingsStore,
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
        request: ComputeSquashPreview,
        source: &FakeDiffSource,
    ) -> Result<ComputeSquashPreviewResponse, ComputeSquashPreviewError> {
        execute(request, &FixedUserSettingsStore::default(), source)
    }

    #[test]
    fn viewer_and_raw_computation_share_the_same_view() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let settings = AppSettings::default();
        let request = ComputeSquashPreview {
            cwd: PathBuf::from("/repo"),
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
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
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
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
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
    fn computes_the_squash_view_from_the_upstream() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
                pinned: None,
            },
            &source,
        )
        .expect("compute succeeds");

        assert_eq!(response.view.title, "squash-preview");
        assert_eq!(response.view.upstream, "origin/main");
        assert_eq!(response.view.files.len(), 1);
    }

    #[test]
    fn pinned_squash_preview_computes_without_an_upstream() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None, // symbolic squash preview errors with "no upstream"
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let response = execute_default_settings(
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
                pinned: Some(PinnedRange {
                    base: "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd".into(),
                    head: "1111111111222222222233333333334444444444".into(),
                }),
            },
            &source,
        )
        .expect("pinned squash compute succeeds");

        assert_eq!(response.view.upstream, "aaaaaaaaaa");
        assert_eq!(response.view.cmd.range, "aaaaaaaaaa..1111111111");
    }

    #[test]
    fn missing_upstream_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            ..Default::default()
        };

        let error = execute_default_settings(
            ComputeSquashPreview {
                cwd: PathBuf::from("/repo"),
                pinned: None,
            },
            &source,
        )
        .expect_err("no upstream errors");

        let ComputeSquashPreviewError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "no upstream");
    }
}
