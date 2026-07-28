//! The `render_squash_preview` vertical slice: render the read-only preview of
//! what `squash-local` would collapse the current branch's unpushed commits
//! into, carrying every user-facing message out as [`Note`]s. Mirrors
//! `render_merge_diff`'s shape; the cli's `gtl squash-preview` calls this
//! in-process.

use std::path::{Path, PathBuf};

use domain::diffs::DiffKind;
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        compute_squash_preview::{self, ComputeSquashPreview},
        util::exclusion_note,
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, GitClient, HtmlRenderer, UserSettingsStore},
    shared::notes::Note,
};

/// Render the squash-preview of the current branch's unpushed commits. The base
/// is always the configured upstream, and the repository is resolved from `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSquashPreview {
    pub cwd: PathBuf,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSquashPreviewOk {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering a squash-preview.
#[derive(Debug, thiserror::Error)]
pub enum RenderSquashPreviewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders a squash preview through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderSquashPreview,
    app_settings: &impl UserSettingsStore,
    source: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderSquashPreviewOk, RenderSquashPreviewError> {
    let RenderSquashPreview { cwd } = req;
    let settings = app_settings.load();
    let built = compute_squash_preview::compute(
        ComputeSquashPreview { cwd, pinned: None },
        &settings,
        source,
    )?;
    let store_root = super::artifacts::root(Path::new(&built.top));
    let view = built.view;
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let render_options = settings.viewer_render_options();
    let theme = settings.theme().map(str::to_owned);
    let html = renderer.build_html(&view, render_options, theme.as_deref());

    let meta = ArtifactMeta {
        repo_root: PathBuf::from(&built.top),
        repo_name: view.repo_name.clone(),
        // ! WorkTree by design: squash-preview is base→working-tree, not a commit range,
        // ! so it is intentionally excluded from range-dedup in the store.
        kind: DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: source
            .resolve_sha(Path::new(&built.top), "HEAD")
            .unwrap_or_default(),
        range_label: built.log_range.clone(),
        head_committed_at: source.committed_at(Path::new(&built.top), "HEAD"),
        generated_at: clock.now_iso(),
        title: "squash-preview".to_string(),
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
    notes.extend(exclusion_note("squash-preview", &view));
    notes.push(Note::info(format!(
        "squash-preview: {commit_count} unpushed commit(s), {file_count} file(s)",
    )));
    notes.push(Note::info(format!("wrote {}", placed.path.display())));
    Ok(RenderSquashPreviewOk {
        artifact: placed.path,
        reused: placed.reused,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::DiffExclusions;

    use super::{RenderSquashPreview, RenderSquashPreviewError, execute};
    use crate::{
        ports::AppSettings,
        shared::notes::Note,
        testing::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(source_top: &str) -> RenderSquashPreview {
        RenderSquashPreview {
            cwd: PathBuf::from(source_top),
        }
    }

    #[test]
    fn renders_with_the_plural_collapse_note() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            known_revs: vec!["origin/main".into()],
            commits: vec![commit("abc1234"), commit("def5678")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo"),
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
                Note::info("squash-preview: 2 unpushed commit(s), 1 file(s)"),
                Note::info("wrote /repo/.artifacts/gtl/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "squash-preview");
    }

    #[test]
    fn render_uses_the_settings_theme_and_resolved_project_exclusions() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
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
            RenderSquashPreview {
                cwd: PathBuf::from("/repo"),
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
        assert!(artifact.html.contains("night"));
    }

    #[test]
    fn no_upstream_is_an_error() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let error = execute(
            req("/repo"),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect_err("missing upstream errors");

        let RenderSquashPreviewError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "no upstream");
    }
}
