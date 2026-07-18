//! The `render_squash_preview` vertical slice: render the read-only preview of
//! what `squash-local` would collapse the current branch's unpushed commits
//! into, carrying every user-facing message out as [`Note`]s. Mirrors
//! `render_merge_diff`'s shape; the cli's `gtl squash-preview` calls this
//! in-process.

use std::path::{Path, PathBuf};

use domain::diffs::{AppliedExclusions, DiffExclusions, DiffKind};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        Cmd, Foot, PinnedRange, View,
        range::DiffRanges,
        util::{DiffData, assemble, exclusion_note, repo_name},
    },
    ports::{AppSettingsStore, ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render the squash-preview of the current branch's unpushed commits (base is
/// always the configured upstream) under `store_root`, resolving the repo from
/// `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSquashPreview {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSquashPreviewResponse {
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
    app_settings: &impl AppSettingsStore,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderSquashPreviewResponse, RenderSquashPreviewError> {
    let RenderSquashPreview { cwd, store_root } = req;
    let settings = app_settings.load();
    let built = build_squash_view(
        source,
        &cwd,
        None,
        settings.theme().map(str::to_owned),
        settings.diff_exclusions(),
    )?;
    let view = built.view;
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = renderer.build_html(&view);

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
        theme: settings.theme().map(str::to_owned),
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
    Ok(RenderSquashPreviewResponse {
        artifact: placed.path,
        reused: placed.reused,
        notes,
    })
}

/// The computed squash-preview view plus the facts the artifact path still needs.
pub(crate) struct SquashViewBuild {
    pub view: View,
    pub top: String,
    pub log_range: String,
}

/// Shared with `compute_squash_preview`: builds the squash-preview [`View`]
/// for the repo at `cwd` (base is always the configured upstream, unless
/// `pinned` supplies a resolved SHA range). No HTML, no store.
pub(crate) fn build_squash_view(
    source: &impl DiffSource,
    cwd: &Path,
    pinned: Option<&PinnedRange>,
    theme: Option<String>,
    exclusions: &DiffExclusions,
) -> anyhow::Result<SquashViewBuild> {
    let top = source.top_level(cwd)?;
    let branch = source.current_branch(Path::new(&top))?;
    let repo_name = repo_name(&top);
    let excluded = exclusions.for_project_or_default(&repo_name);

    let (upstream, io_ranges, view_ranges) = if let Some(pin) = pinned {
        (
            pin.display_base(),
            DiffRanges::exact(pin.git_range()),
            DiffRanges::exact(pin.display_range()),
        )
    } else {
        let upstream = source.upstream(Path::new(&top))?;
        let symbolic = DiffRanges::unpushed(&upstream);
        (upstream, symbolic.clone(), symbolic)
    };

    let DiffData {
        commits,
        files,
        hidden_paths,
    } = assemble(
        source,
        Path::new(&top),
        &io_ranges.diff,
        &io_ranges.log,
        excluded,
    )?;

    let view = View {
        repo_name,
        repo_root: top.clone(),
        branch,
        upstream,
        title: "squash-preview".to_string(),
        cmd: Cmd {
            lead: "git log ".to_string(),
            range: view_ranges.log.clone(),
            trail: " --stat".to_string(),
        },
        commits_label: "# commits — collapse into 1".to_string(),
        foot: Foot {
            cmd: "squash-local".to_string(),
            note: collapse_note(commits.len()),
        },
        commits,
        files,
        theme,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    Ok(SquashViewBuild {
        view,
        top,
        log_range: io_ranges.log,
    })
}

fn collapse_note(commit_count: usize) -> String {
    if commit_count == 1 {
        "# would collapse this commit into one — read-only preview".to_string()
    } else {
        format!("# would collapse these {commit_count} commits into one — read-only preview")
    }
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
            FakeDiffSource, FixedAppSettingsStore, FixedClock, InMemoryArtifactStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(source_top: &str) -> RenderSquashPreview {
        RenderSquashPreview {
            cwd: PathBuf::from(source_top),
            store_root: PathBuf::from("/store"),
        }
    }

    #[test]
    fn renders_with_the_plural_collapse_note() {
        let source = FakeDiffSource {
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
            &FixedAppSettingsStore::default(),
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
                Note::info("squash-preview: 2 unpushed commit(s), 1 file(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "squash-preview");
    }

    #[test]
    fn render_uses_the_settings_theme_and_resolved_project_exclusions() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let app_settings = FixedAppSettingsStore::new(AppSettings::new(
            Some("night".into()),
            true,
            DiffExclusions::new([("repo".to_string(), vec!["md"])], None),
        ));

        execute(
            RenderSquashPreview {
                cwd: PathBuf::from("/repo"),
                store_root: PathBuf::from("/store"),
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
    fn no_upstream_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let error = execute(
            req("/repo"),
            &FixedAppSettingsStore::default(),
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
