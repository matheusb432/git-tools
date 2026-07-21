//! The `render_merge_diff` vertical slice: render the diff of everything on
//! the current branch that would merge into `base`, carrying every
//! user-facing message out as [`Note`]s. Mirrors `render_diff`'s shape; the
//! cli's `gtl merge-diff` calls this in-process.

use std::path::{Path, PathBuf};

use domain::diffs::DiffKind;
use serde::{Deserialize, Serialize};

pub use crate::diffs::compute_merge_diff::DEFAULT_BASE;
use crate::{
    diffs::{
        compute_merge_diff::{self, ComputeMergeDiff},
        range_view::TITLE_MERGE_DIFF,
        util::exclusion_note,
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer, UserSettingsStore},
    shared::notes::Note,
};

/// Render the merge-diff of the current branch into `base` (default `main`)
/// under `store_root`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderMergeDiff {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
    #[serde(default)]
    pub base: Option<String>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMergeDiffResponse {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering a merge-diff.
#[derive(Debug, thiserror::Error)]
pub enum RenderMergeDiffError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders a merge diff through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderMergeDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderMergeDiffResponse, RenderMergeDiffError> {
    let RenderMergeDiff {
        cwd,
        store_root,
        base,
    } = req;
    let settings = app_settings.load();
    let built = compute_merge_diff::compute(
        ComputeMergeDiff {
            cwd,
            base,
            pinned: None,
        },
        &settings,
        source,
    )?;
    let view = built.view;
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let render_options = settings.viewer_render_options();
    let theme = settings.theme().map(str::to_owned);
    let html = renderer.build_html(&view, render_options, theme.as_deref());

    let meta = ArtifactMeta {
        repo_root: PathBuf::from(&built.top),
        repo_name: view.repo_name.clone(),
        kind: DiffKind::from_diff_range(&built.diff_range),
        base_sha: source
            .resolve_sha(Path::new(&built.top), &built.base)
            .unwrap_or_default(),
        head_sha: source
            .resolve_sha(Path::new(&built.top), "HEAD")
            .unwrap_or_default(),
        range_label: built.diff_range.clone(),
        head_committed_at: source.committed_at(Path::new(&built.top), "HEAD"),
        generated_at: clock.now_iso(),
        title: TITLE_MERGE_DIFF.to_string(),
        render_options,
        theme,
        excluded_extensions: settings
            .diff_exclusions()
            .for_project_or_default(&view.repo_name)
            .extensions()
            .to_vec(),
    };
    let placed = store.place(&store_root, &meta, &html)?;

    let mut notes = Vec::new();
    notes.extend(exclusion_note(TITLE_MERGE_DIFF, &view));
    notes.push(Note::info(format!(
        "{TITLE_MERGE_DIFF}: {commit_count} commit{} to merge into {}, {file_count} file{}",
        plural(commit_count),
        built.base,
        plural(file_count),
    )));
    notes.push(Note::info(format!("wrote {}", placed.path.display())));
    Ok(RenderMergeDiffResponse {
        artifact: placed.path,
        reused: placed.reused,
        notes,
    })
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::DiffExclusions;

    use super::{RenderMergeDiff, RenderMergeDiffError, execute};
    use crate::{
        ports::AppSettings,
        shared::notes::Note,
        testing::{
            FakeDiffSource, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore,
            StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(source_top: &str, base: Option<&str>) -> RenderMergeDiff {
        RenderMergeDiff {
            cwd: PathBuf::from(source_top),
            store_root: PathBuf::from("/store"),
            base: base.map(str::to_string),
        }
    }

    #[test]
    fn renders_and_stores_an_artifact_with_the_summary_notes() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", None),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.artifact,
            PathBuf::from("/store/diffs/fake/artifact.html")
        );
        assert!(!response.reused);
        assert_eq!(
            response.notes,
            vec![
                Note::info("merge-diff: 1 commit to merge into main, 1 file"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "merge-diff");
        assert_eq!(artifact.meta.repo_name, "repo");
    }

    #[test]
    fn render_uses_the_settings_theme_and_resolved_project_exclusions() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let app_settings = FixedUserSettingsStore::new(AppSettings::new(
            Some("night".into()),
            true,
            DiffExclusions::new([("repo".to_string(), vec!["md"])], None),
        ));

        execute(
            RenderMergeDiff {
                cwd: PathBuf::from("/repo"),
                store_root: PathBuf::from("/store"),
                base: None,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.excluded_extensions, vec!["md"]);
        assert!(artifact.html.contains("night"));
    }

    #[test]
    fn blank_base_falls_back_to_default() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", Some("   ")),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.notes[0],
            Note::info("merge-diff: 0 commits to merge into main, 0 files")
        );
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let error = execute(
            req("/repo", Some("nope")),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect_err("unknown base errors");

        let RenderMergeDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
