//! The `render_merge_diff` vertical slice: render the diff of everything on
//! the current branch that would merge into `base`, carrying every
//! user-facing message out as [`Note`]s. Mirrors `render_diff`'s shape; the
//! cli's `gtl merge-diff` calls this in-process.

use std::path::{Path, PathBuf};

use gtl_models::diffs::DiffKind;
use serde::{Deserialize, Serialize};

pub use crate::diffs::compute_merge_diff::DEFAULT_BASE;
use crate::{
    diffs::{
        compute_merge_diff::{self, ComputeMergeDiff},
        logic::{exclusions, range_view::TITLE_MERGE_DIFF},
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, GitClient, HtmlRenderer, UserSettingsStore},
    shared::notes::Note,
};

/// Render the merge-diff of the current branch into `base` (default `main`),
/// resolving the repository from `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderMergeDiff {
    pub cwd: PathBuf,
    #[serde(default)]
    pub base: Option<String>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMergeDiffOk {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering a merge-diff.
#[derive(Debug, thiserror::Error)]
pub enum RenderMergeDiffError {
    #[error(transparent)]
    Compute(#[from] compute_merge_diff::ComputeMergeDiffError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders a merge diff through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderMergeDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderMergeDiffOk, RenderMergeDiffError> {
    let RenderMergeDiff { cwd, base } = req;
    let computed = compute_merge_diff::execute(
        ComputeMergeDiff {
            cwd,
            base,
            pinned: None,
        },
        app_settings,
        source,
    )?;
    let store_root = super::logic::artifacts::root(Path::new(&computed.top));
    let view = computed.view;
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = renderer.build_html(&view, computed.render_options, computed.theme.as_deref());

    let meta = ArtifactMeta {
        repo_root: PathBuf::from(&computed.top),
        repo_name: view.repo_name.clone(),
        kind: DiffKind::from_diff_range(&computed.diff_range),
        base_sha: source
            .resolve_sha(Path::new(&computed.top), &computed.base)
            .unwrap_or_default(),
        head_sha: source
            .resolve_sha(Path::new(&computed.top), "HEAD")
            .unwrap_or_default(),
        range_label: computed.diff_range.clone(),
        head_committed_at: source.committed_at(Path::new(&computed.top), "HEAD"),
        generated_at: clock.now_iso(),
        title: TITLE_MERGE_DIFF.to_string(),
        render_options: computed.render_options,
        theme: computed.theme,
        excluded_extensions: computed.excluded_extensions,
    };
    let placed = store.place(&store_root, &meta, &html)?;

    let mut notes = Vec::new();
    notes.extend(exclusions::note(TITLE_MERGE_DIFF, &view));
    notes.push(Note::info(format!(
        "{TITLE_MERGE_DIFF}: {commit_count} commit{} to merge into {}, {file_count} file{}",
        plural(commit_count),
        computed.base,
        plural(file_count),
    )));
    notes.push(Note::info(format!("wrote {}", placed.path.display())));
    Ok(RenderMergeDiffOk {
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

    use gtl_models::{
        diffs::DiffExclusions,
        settings::UserSettings,
        viewer::{RenderOptions, Theme},
    };

    use super::{RenderMergeDiff, RenderMergeDiffError, execute};
    use crate::{
        diffs::compute_merge_diff::{self, ComputeMergeDiff},
        shared::notes::Note,
        testing::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(source_top: &str, base: Option<&str>) -> RenderMergeDiff {
        RenderMergeDiff {
            cwd: PathBuf::from(source_top),
            base: base.map(str::to_string),
        }
    }

    #[test]
    fn renders_and_stores_an_artifact_with_the_summary_notes() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let computed = compute_merge_diff::execute(
            ComputeMergeDiff {
                cwd: PathBuf::from("/repo"),
                base: None,
                pinned: None,
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("compute succeeds");

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
            PathBuf::from("/repo/.artifacts/gtl/artifact.html")
        );
        assert!(!response.reused);
        assert_eq!(
            response.notes,
            vec![
                Note::info("merge-diff: 1 commit to merge into main, 1 file"),
                Note::info("wrote /repo/.artifacts/gtl/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "merge-diff");
        assert_eq!(artifact.meta.repo_name, "repo");
        assert_eq!(artifact.meta.repo_root, PathBuf::from(computed.top));
        assert_eq!(artifact.meta.range_label, computed.diff_range);
        assert_eq!(artifact.meta.render_options, computed.render_options);
        assert_eq!(artifact.meta.theme, computed.theme);
        assert_eq!(
            artifact.meta.excluded_extensions,
            computed.excluded_extensions
        );
    }

    #[test]
    fn render_uses_the_settings_theme_and_resolved_project_exclusions() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let app_settings = FixedUserSettingsStore::new(UserSettings::new(
            Some(Theme::Noir),
            RenderOptions::DEFAULT,
            true,
            DiffExclusions::new([("repo".to_string(), vec!["md"])], None),
        ));

        execute(
            RenderMergeDiff {
                cwd: PathBuf::from("/repo"),
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
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.excluded_extensions, vec!["md"]);
        assert!(artifact.html.contains("noir"));
    }

    #[test]
    fn blank_base_falls_back_to_default() {
        let source = FakeGitClient {
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
        let source = FakeGitClient {
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

        let RenderMergeDiffError::Compute(compute_merge_diff::ComputeMergeDiffError::Unexpected(
            err,
        )) = error
        else {
            panic!("expected merge computation error");
        };
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
