//! The `render_merge_diff` vertical slice: render the diff of everything on
//! the current branch that would merge into `base`, carrying every
//! user-facing message out as [`Note`]s. Mirrors `render_diff`'s shape; the
//! cli's `gtl merge-diff` calls this in-process.

use std::path::PathBuf;

use gtl_models::{
    artifacts::ArtifactDiffIdentity,
    diffs::{DiffKind, PinnedRange},
    failure::ErrorMeta,
    git::GitRevision,
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        compute_merge_diff::{self, ComputeMergeDiff},
        extension_filter_note,
        range_view::TITLE_MERGE_DIFF,
    },
    ports::{
        ArtifactMeta, ArtifactStore, Clock, ExtensionFilterReader, GitClient, HtmlRenderer,
        PlacedArtifact, UserSettingsReader,
    },
    shared::notes::Note,
};

/// Render the merge-diff of the current branch into `base` (default `main`),
/// resolving the repository from `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderMergeDiff {
    pub cwd: PathBuf,
    #[serde(default)]
    pub base: Option<GitRevision>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMergeDiffOk {
    pub placement: PlacedArtifact,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering a merge-diff.
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum RenderMergeDiffError {
    #[error(transparent)]
    #[meta(transparent)]
    Compute(#[from] compute_merge_diff::ComputeMergeDiffError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Renders a merge diff through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderMergeDiff,
    app_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
    filters: &impl ExtensionFilterReader,
) -> Result<RenderMergeDiffOk, RenderMergeDiffError> {
    let RenderMergeDiff { cwd, base } = req;
    let repo_root = git.top_level(&cwd)?;
    let computed = compute_merge_diff::execute(
        ComputeMergeDiff {
            repo_root,
            base,
            pinned: None,
            changes_since: None,
        },
        app_settings,
        git,
        filters,
    )?;
    let store_root = super::artifacts::root(computed.top.as_ref());
    let view = computed.view;
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = renderer.build_html(
        &view,
        computed.render_options,
        computed.theme,
        computed.language,
    )?;
    let commit_range = git
        .resolve_commit_id(&computed.top, &computed.base)
        .ok()
        .zip(
            git.resolve_commit_id(&computed.top, &GitRevision::head())
                .ok(),
        )
        .map(|(base, head)| PinnedRange { base, head });

    let meta = ArtifactMeta {
        repo_root: computed.top.clone(),
        identity: ArtifactDiffIdentity::from_parts(
            DiffKind::from_diff_range(computed.diff_range.as_arg()),
            commit_range,
        )
        .map_err(anyhow::Error::from)?,
        generated_at: clock.now().map_err(anyhow::Error::from)?,
        render_options: computed.render_options,
        theme: computed.theme,
        language: computed.language,
        extension_filter: computed.extension_filter,
    };
    let placed = store.place(&store_root, &meta, &html)?;

    let mut notes = Vec::new();
    notes.extend(extension_filter_note::note(TITLE_MERGE_DIFF, &view));
    notes.push(Note::info(format!(
        "{TITLE_MERGE_DIFF}: {commit_count} commit{} to merge into {}, {file_count} file{}",
        plural(commit_count),
        computed.base,
        plural(file_count),
    )));
    notes.push(Note::info(format!("wrote {}", placed.path().display())));
    Ok(RenderMergeDiffOk {
        placement: placed,
        notes,
    })
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{settings::UserSettings, viewer::Theme};

    use super::{RenderMergeDiff, RenderMergeDiffError};
    use crate::{
        diffs::{
            compute_merge_diff::{self, ComputeMergeDiff},
            render_merge_diff,
        },
        shared::notes::Note,
        utils::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore,
            SavedExtensionFilters, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
            hiding_extensions, repository_root,
        },
    };

    fn req(source_top: &str, base: Option<&str>) -> RenderMergeDiff {
        RenderMergeDiff {
            cwd: PathBuf::from(source_top),
            base: base.map(crate::utils::git_revision),
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
                repo_root: crate::utils::repository_root("/repo"),
                base: None,
                pinned: None,
                changes_since: None,
            },
            &FixedUserSettingsStore::default(),
            &source,
            &SavedExtensionFilters::default(),
        )
        .unwrap();

        let response = render_merge_diff::execute(
            req("/repo", None),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &SavedExtensionFilters::default(),
        )
        .unwrap();

        assert_eq!(
            response.placement.path().as_path(),
            PathBuf::from("/repo/.artifacts/gtl/artifact.html")
        );
        assert!(!response.placement.is_reused());
        assert_eq!(
            response.notes,
            vec![
                Note::info("merge-diff: 1 commit to merge into main, 1 file"),
                Note::info("wrote /repo/.artifacts/gtl/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(artifact.meta.repo_root, computed.top);
        assert_eq!(artifact.meta.render_options, computed.render_options);
        assert_eq!(artifact.meta.theme, computed.theme);
        assert_eq!(artifact.meta.extension_filter, computed.extension_filter);
    }

    #[test]
    fn render_uses_the_settings_theme_language_and_saved_repository_filter() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let app_settings = FixedUserSettingsStore::new(
            UserSettings::default()
                .with_theme(Some(Theme::Graphite))
                .with_language(gtl_models::settings::ViewerLanguage::PtBr),
        );

        render_merge_diff::execute(
            RenderMergeDiff {
                cwd: PathBuf::from("/repo"),
                base: None,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &SavedExtensionFilters::new([(repository_root("/repo"), hiding_extensions(&["md"]))]),
        )
        .unwrap();

        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(artifact.meta.extension_filter, hiding_extensions(&["md"]));
        assert!(artifact.html.contains("graphite"));
        assert_eq!(
            artifact.meta.language,
            gtl_models::settings::ViewerLanguage::PtBr
        );
        assert!(artifact.html.contains("lang=\"pt-BR\""));
    }

    #[test]
    fn absent_base_falls_back_to_default() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = render_merge_diff::execute(
            req("/repo", None),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &SavedExtensionFilters::default(),
        )
        .unwrap();

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

        let error = render_merge_diff::execute(
            req("/repo", Some("nope")),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &SavedExtensionFilters::default(),
        )
        .unwrap_err();

        let err = match error {
            RenderMergeDiffError::Compute(
                compute_merge_diff::ComputeMergeDiffError::Unexpected(error),
            ) => Some(error),
            RenderMergeDiffError::Compute(compute_merge_diff::ComputeMergeDiffError::Settings(
                _,
            ))
            | RenderMergeDiffError::Unexpected(_) => None,
        }
        .unwrap();
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
