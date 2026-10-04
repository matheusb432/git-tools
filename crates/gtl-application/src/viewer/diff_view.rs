use std::{ops::Deref, sync::Arc};

use gtl_models::paths::RepositoryRelativePath;
use gtl_wire::viewer::{
    ViewerActiveView, ViewerAppliedExtensionFilter, ViewerCommandLine, ViewerCommitSelection,
    ViewerCommitSummary, ViewerDiffDensity, ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary,
    ViewerFooter, ViewerRenderOptions, ViewerRowContentId, ViewerTheme, ViewerViewIdentity,
};
use sha2::{Digest as _, Sha256};

use crate::{
    diffs::{FileDiff, FileStatus, View, source_lines::DiffSourceLines},
    viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
};

const GIANT_FILE_CHARACTERS: usize = 250_000;

/// Immutable row sources and their digests, shared across shell and selection snapshots.
#[derive(Debug, Clone)]
pub struct ViewerDiffSnapshot(Arc<ViewerDiffSnapshotContent>);

#[derive(Debug)]
struct ViewerDiffSnapshotContent {
    view: Arc<View>,
    compact: [u8; 32],
    full: [u8; 32],
    files: Vec<([u8; 32], [u8; 32])>,
}

impl ViewerDiffSnapshot {
    #[must_use]
    pub fn new(view: Arc<View>) -> Self {
        let (compact, compact_files) = source_digests(&view, ViewerDiffDensity::Compact);
        let (full, full_files) = if view.files.iter().all(|file| file.full_lines.is_none()) {
            (compact, compact_files.clone())
        } else {
            source_digests(&view, ViewerDiffDensity::Full)
        };
        Self(Arc::new(ViewerDiffSnapshotContent {
            view,
            compact,
            full,
            files: compact_files.into_iter().zip(full_files).collect(),
        }))
    }

    fn file_content_id(
        &self,
        index: usize,
        options: ViewerRenderOptions,
    ) -> Option<ViewerRowContentId> {
        self.0.files.get(index).map(|(compact, full)| {
            row_content_id(
                match options.density {
                    ViewerDiffDensity::Compact => *compact,
                    ViewerDiffDensity::Full => *full,
                },
                options.layout,
            )
        })
    }

    #[must_use]
    pub fn shared_view(&self) -> Arc<View> {
        Arc::clone(&self.0.view)
    }

    #[must_use]
    pub fn content_id(&self, options: ViewerRenderOptions) -> ViewerRowContentId {
        let source = match options.density {
            ViewerDiffDensity::Compact => self.0.compact,
            ViewerDiffDensity::Full => self.0.full,
        };
        row_content_id(source, options.layout)
    }
}

impl Deref for ViewerDiffSnapshot {
    type Target = View;

    fn deref(&self) -> &Self::Target {
        &self.0.view
    }
}

fn source_digests(view: &View, density: ViewerDiffDensity) -> ([u8; 32], Vec<[u8; 32]>) {
    let files = view
        .files
        .iter()
        .map(|file| {
            let mut digest = Sha256::new();
            digest.update(b"gtl.viewer.file-source.v2\0");
            hash_frame(
                &mut digest,
                file.path.as_path().as_os_str().as_encoded_bytes(),
            );
            let lines = selected_lines(file, density);
            digest.update((lines.len() as u64).to_be_bytes());
            digest.update(lines.fingerprint());
            digest.finalize().into()
        })
        .collect::<Vec<[u8; 32]>>();
    let mut digest = Sha256::new();
    digest.update(b"gtl.viewer.row-sources.v2\0");
    digest.update((files.len() as u64).to_be_bytes());
    for file in &files {
        digest.update(file);
    }
    (digest.finalize().into(), files)
}

fn hash_frame(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

fn row_content_id(
    source: [u8; 32],
    layout: gtl_wire::viewer::ViewerDiffLayout,
) -> ViewerRowContentId {
    let mut digest = Sha256::new();
    digest.update(b"gtl.viewer.rows.v3\0");
    digest.update([match layout {
        gtl_wire::viewer::ViewerDiffLayout::Unified => 0,
        gtl_wire::viewer::ViewerDiffLayout::Split => 1,
    }]);
    digest.update(source);
    ViewerRowContentId::from_digest(digest.finalize().into())
}

/// Borrowed source selected for one identity-bound viewer file.
pub struct ViewerDiffFileSource<'view> {
    pub path: &'view RepositoryRelativePath,
    pub lines: &'view DiffSourceLines,
}

/// Projects validated application rendering options into the shared client contract.
#[must_use]
pub const fn project_render_options(options: RenderOptions) -> ViewerRenderOptions {
    ViewerRenderOptions {
        wrap_lines: options.wrap_lines(),
        layout: match options.layout() {
            DiffLayout::Unified => gtl_wire::viewer::ViewerDiffLayout::Unified,
            DiffLayout::Split => gtl_wire::viewer::ViewerDiffLayout::Split,
        },
        density: match options.density() {
            DiffDensity::Compact => ViewerDiffDensity::Compact,
            DiffDensity::Full => ViewerDiffDensity::Full,
        },
    }
}

/// Projects a validated application theme into the shared client contract.
#[must_use]
pub const fn project_theme(theme: Theme) -> ViewerTheme {
    match theme {
        Theme::Dark => ViewerTheme::Dark,
        Theme::Mirage => ViewerTheme::Mirage,
        Theme::Glacier => ViewerTheme::Glacier,
        Theme::Graphite => ViewerTheme::Graphite,
        Theme::Carbon => ViewerTheme::Carbon,
    }
}

/// Returns the stable document anchor for a file in a rendered diff view.
#[must_use]
pub fn diff_file_anchor_id(path: &RepositoryRelativePath) -> String {
    let mut body = String::new();
    let mut last_was_separator = false;

    for character in path.to_string_lossy().chars() {
        if character.is_ascii_alphanumeric() {
            body.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            body.push('-');
            last_was_separator = true;
        }
    }

    format!("f-{}", body.trim_matches('-'))
}

/// Projects an application diff view into the shared client-rendering contract.
///
/// `view` supplies the displayed files and command. `range_view` keeps the commit shelf stable
/// while the desktop viewer displays one selected commit.
#[must_use]
pub fn project_diff_view(
    view: &View,
    range_view: &View,
    identity: ViewerViewIdentity,
    commit_selection: ViewerCommitSelection,
) -> ViewerActiveView {
    let snapshot = ViewerDiffSnapshot::new(Arc::new(view.clone()));
    let content_id = snapshot.content_id(identity.render_options);
    let mut projected = project_diff_view_with_content_id(
        &snapshot,
        range_view,
        identity,
        commit_selection,
        content_id,
    );
    projected.commits = range_view
        .commits
        .iter()
        .map(|commit| ViewerCommitSummary {
            id: commit.id.clone(),
            subject: commit.subject.clone(),
            body: commit.body.clone(),
            committed_at: commit.committed_at.clone(),
            is_merge: commit.is_merge(),
        })
        .collect();
    projected
}

pub(super) fn project_diff_view_with_content_id(
    view: &ViewerDiffSnapshot,
    range_view: &View,
    identity: ViewerViewIdentity,
    commit_selection: ViewerCommitSelection,
    content_id: ViewerRowContentId,
) -> ViewerActiveView {
    ViewerActiveView {
        modified_files: false,
        row_source: gtl_wire::viewer::ViewerRowSourceState::Ready,
        identity,
        content_id,
        title: view.title.clone(),
        repository_name: view.repo_name.clone(),
        branch: view.branch.clone(),
        upstream: view.upstream.clone(),
        command: ViewerCommandLine {
            lead: view.cmd.lead.clone(),
            range: view.cmd.range.clone(),
            trail: view.cmd.trail.clone(),
        },
        files: view
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let mut summary = project_file(view, identity, index, file);
                summary.source_id = view.file_content_id(index, identity.render_options);
                summary
            })
            .collect(),
        commit_count: range_view.commits.len(),
        commits: Vec::new(),
        commit_selection,
        footer: ViewerFooter {
            command: view.foot.cmd.clone(),
        },
        extension_filter: view.extension_filter.as_ref().map(|applied| {
            ViewerAppliedExtensionFilter {
                filter: applied.filter.clone(),
                hidden_paths: applied.hidden_paths.clone(),
            }
        }),
        changes_since: None,
    }
}

/// Resolves one opaque file ID and applies the requested compact/full source choice.
#[must_use]
pub fn viewer_diff_file_source<'view>(
    view: &'view View,
    id: &ViewerDiffFileId,
    density: ViewerDiffDensity,
) -> Option<ViewerDiffFileSource<'view>> {
    let file = file_by_id(view, id)?;
    Some(ViewerDiffFileSource {
        path: &file.path,
        lines: selected_lines(file, density),
    })
}

fn project_file(
    view: &View,
    identity: ViewerViewIdentity,
    index: usize,
    file: &FileDiff,
) -> ViewerFileSummary {
    let status = file.status();
    ViewerFileSummary {
        review: Some(crate::diffs::review::file_review(&view.repo_root, file)),
        source_id: None,
        id: ViewerDiffFileId::for_index(index),
        path: file.path.clone(),
        absolute_path: view.repo_root.join(&file.path),
        anchor_id: diff_file_anchor_id(&file.path),
        added: file.added,
        removed: file.removed,
        status: viewer_file_status(status),
        can_open_in_editor: status != FileStatus::Deleted,
        row_count: super::rows::viewer_file_row_count(
            selected_lines(file, identity.render_options.density),
            identity.render_options.layout,
        ),
        initially_expanded: selected_lines(file, identity.render_options.density).text_bytes()
            <= GIANT_FILE_CHARACTERS,
    }
}

pub(super) fn file_by_id<'view>(
    view: &'view View,
    id: &ViewerDiffFileId,
) -> Option<&'view FileDiff> {
    let index = id.as_str().strip_prefix("file-")?.parse::<usize>().ok()?;
    if ViewerDiffFileId::for_index(index) != *id {
        return None;
    }
    view.files.get(index)
}

fn selected_lines(file: &FileDiff, density: ViewerDiffDensity) -> &DiffSourceLines {
    match (density, file.full_lines.as_ref()) {
        (ViewerDiffDensity::Full, Some(lines)) => lines,
        (ViewerDiffDensity::Compact | ViewerDiffDensity::Full, _) => &file.lines,
    }
}

const fn viewer_file_status(status: FileStatus) -> ViewerFileStatus {
    match status {
        FileStatus::Added => ViewerFileStatus::Added,
        FileStatus::Deleted => ViewerFileStatus::Deleted,
        FileStatus::Renamed => ViewerFileStatus::Renamed,
        FileStatus::Modified => ViewerFileStatus::Modified,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::{AppliedExtensionFilter, CommitIdAbbreviation, DiffLineCount},
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId},
    };
    use gtl_wire::viewer::{
        ViewerCommitSelection, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions,
        ViewerTheme, ViewerViewIdentity,
    };

    use super::{diff_file_anchor_id, project_diff_view, project_render_options, project_theme};
    use crate::{
        diffs::{Cmd, FileDiff, View},
        utils,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    fn identity(density: ViewerDiffDensity) -> ViewerViewIdentity {
        ViewerViewIdentity {
            tab_id: ViewerTabId::try_new(7).unwrap(),
            range_generation: ViewerRangeGeneration::new(8),
            selection_generation: ViewerSelectionGeneration::new(9),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density,
            },
        }
    }

    #[test]
    fn file_sources_keep_their_identity_when_other_files_are_hidden() {
        use std::sync::Arc;
        let original = view();
        for density in [ViewerDiffDensity::Compact, ViewerDiffDensity::Full] {
            let options = identity(density).render_options;
            let all = super::ViewerDiffSnapshot::new(Arc::new(original.clone()));
            let mut filtered = original.clone();
            filtered.files.remove(0);
            let filtered = super::ViewerDiffSnapshot::new(Arc::new(filtered));
            assert_ne!(all.content_id(options), filtered.content_id(options));
            assert_eq!(
                all.file_content_id(1, options),
                filtered.file_content_id(0, options)
            );
        }
    }

    fn view() -> View {
        View {
            file_filter: crate::diffs::file_filter::DiffFileFilter::default(),
            repo_name: utils::project_name("git-tools"),
            commits: vec![utils::diffs::commit_with(
                "0123456789abcdef0123456789abcdef01234567",
                "subject",
                &[
                    "1111111111111111111111111111111111111111",
                    "2222222222222222222222222222222222222222",
                ],
            )],
            files: vec![
                FileDiff {
                    path: utils::repository_relative_path("src/a b.rs"),
                    added: DiffLineCount::new(2),
                    removed: DiffLineCount::new(1),
                    lines: vec!["new file mode 100644".into(), "+compact".into()].into(),
                    full_lines: Some(vec!["new file mode 100644".into(), "+full".into()].into()),
                },
                FileDiff {
                    path: utils::repository_relative_path("removed.rs"),
                    added: DiffLineCount::default(),
                    removed: DiffLineCount::new(1),
                    lines: vec!["deleted file mode 100644".into()].into(),
                    full_lines: None,
                },
            ],
            title: crate::utils::diffs::view_title("Feature diff"),
            cmd: Cmd {
                lead: "git diff ".into(),
                range: "main...feature".into(),
                trail: " --".into(),
            },
            extension_filter: Some(AppliedExtensionFilter {
                filter: utils::hiding_extensions(&["lock"]),
                hidden_paths: vec![utils::repository_relative_path("Cargo.lock")],
            }),
            ..utils::diffs::view()
        }
    }

    #[test]
    fn file_anchor_normalizes_paths_and_keeps_a_stable_prefix() {
        assert_eq!(
            diff_file_anchor_id(&utils::repository_relative_path("src/a b.rs")),
            "f-src-a-b-rs"
        );
        assert_eq!(
            diff_file_anchor_id(&utils::repository_relative_path("---")),
            "f-"
        );
    }

    fn content_id(view: &View, density: ViewerDiffDensity) -> gtl_wire::viewer::ViewerRowContentId {
        project_diff_view(view, view, identity(density), ViewerCommitSelection::None).content_id
    }

    #[test]
    fn row_content_is_equal_across_tab_metadata_and_snapshot_projection() {
        let original = view();
        let mut renamed = original.clone();
        renamed.title = crate::utils::diffs::view_title("Live unpushed commits");
        renamed.cmd.lead = "gtl live ".into();
        renamed.cmd.range = "0123456..abcdef0".into();
        renamed.foot.cmd = "different command".into();
        renamed.repo_root =
            utils::repository_root("//fixture.invalid/repositories/another-checkout");
        renamed.commits.clear();
        let mut other_identity = identity(ViewerDiffDensity::Compact);
        other_identity.tab_id = ViewerTabId::try_new(99).unwrap();
        other_identity.range_generation = ViewerRangeGeneration::new(100);
        other_identity.selection_generation = ViewerSelectionGeneration::new(101);
        let static_view = project_diff_view(
            &original,
            &original,
            identity(ViewerDiffDensity::Compact),
            ViewerCommitSelection::None,
        );
        let snapshot = super::ViewerDiffSnapshot::new(std::sync::Arc::new(renamed));
        let live_view = super::project_diff_view_with_content_id(
            &snapshot,
            &original,
            other_identity,
            ViewerCommitSelection::None,
            snapshot.content_id(other_identity.render_options),
        );

        assert_ne!(static_view.identity, live_view.identity);
        assert_eq!(static_view.content_id, live_view.content_id);
        assert_ne!(static_view.title, live_view.title);
    }

    #[test]
    fn same_stats_source_edits_paths_and_file_order_invalidate_row_content() {
        let original = view();
        let expected = content_id(&original, ViewerDiffDensity::Compact);
        let mut edited = original.clone();
        edited.files[0].lines = ["new file mode 100644", "+changed"].into_iter().collect();
        assert_eq!(original.files[0].added, edited.files[0].added);
        assert_eq!(original.files[0].removed, edited.files[0].removed);
        assert_eq!(
            original.files[0].lines.iter().nth(1).unwrap().len(),
            edited.files[0].lines.iter().nth(1).unwrap().len()
        );
        assert_ne!(expected, content_id(&edited, ViewerDiffDensity::Compact));

        let mut renamed = original.clone();
        renamed.files[0].path = utils::repository_relative_path("src/other.rs");
        assert_ne!(expected, content_id(&renamed, ViewerDiffDensity::Compact));
        let mut reordered = original.clone();
        reordered.files.swap(0, 1);
        assert_ne!(expected, content_id(&reordered, ViewerDiffDensity::Compact));
    }

    #[test]
    fn row_content_frames_paths_files_and_ordered_lines_without_ambiguity() {
        let mut first = view();
        first.files.truncate(1);
        let mut second = first.clone();
        for (left, right) in [
            (vec![], vec![""]),
            (vec!["+a"], vec!["+a", ""]),
            (vec!["+ab", "c"], vec!["+a", "bc"]),
            (vec!["+a", "+b"], vec!["+b", "+a"]),
            (vec!["+a\n+b"], vec!["+a", "+b"]),
            (vec!["+a\0+b"], vec!["+a", "\0+b"]),
        ] {
            first.files[0].lines = left.into_iter().map(str::to_owned).collect();
            second.files[0].lines = right.into_iter().map(str::to_owned).collect();
            assert_ne!(
                content_id(&first, ViewerDiffDensity::Compact),
                content_id(&second, ViewerDiffDensity::Compact)
            );
        }
        first.files[0].path = utils::repository_relative_path("ab");
        first.files[0].lines = vec!["c".into()].into();
        second.files[0].path = utils::repository_relative_path("a");
        second.files[0].lines = vec!["bc".into()].into();
        assert_ne!(
            content_id(&first, ViewerDiffDensity::Compact),
            content_id(&second, ViewerDiffDensity::Compact)
        );
    }

    #[test]
    fn row_content_covers_layout_and_only_the_density_selected_source() {
        let original = view();
        let compact = content_id(&original, ViewerDiffDensity::Compact);
        let full = content_id(&original, ViewerDiffDensity::Full);
        assert_ne!(compact, full);
        let mut changed = original.clone();
        changed.files[0].full_lines = Some(["new file mode 100644", "+more"].into_iter().collect());
        assert_eq!(compact, content_id(&changed, ViewerDiffDensity::Compact));
        assert_ne!(full, content_id(&changed, ViewerDiffDensity::Full));
        changed.files[0].full_lines = Some(changed.files[0].lines.clone());
        assert_eq!(compact, content_id(&changed, ViewerDiffDensity::Full));
        changed.files[0].full_lines = None;
        assert_eq!(compact, content_id(&changed, ViewerDiffDensity::Full));
        let snapshot = super::ViewerDiffSnapshot::new(std::sync::Arc::new(original));
        assert_eq!(
            compact,
            snapshot.content_id(identity(ViewerDiffDensity::Compact).render_options)
        );
        assert_eq!(
            full,
            snapshot.content_id(identity(ViewerDiffDensity::Full).render_options)
        );
        let split = ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Split,
            density: ViewerDiffDensity::Compact,
        };
        assert_ne!(compact, snapshot.content_id(split));
    }

    #[test]
    fn snapshot_keeps_source_and_digest_together_when_the_producer_edits_its_view() {
        let mut source = std::sync::Arc::new(view());
        let snapshot = super::ViewerDiffSnapshot::new(std::sync::Arc::clone(&source));
        let options = identity(ViewerDiffDensity::Compact).render_options;
        let expected = snapshot.content_id(options);
        std::sync::Arc::make_mut(&mut source).files[0].lines =
            ["new file mode 100644", "+changed"].into_iter().collect();
        assert_eq!(snapshot.files[0].lines.iter().nth(1).unwrap(), "+compact");
        assert_eq!(snapshot.clone().content_id(options), expected);
        assert_ne!(
            super::ViewerDiffSnapshot::new(source).content_id(options),
            expected
        );
    }

    #[test]
    fn shared_contract_projection_covers_every_render_option_and_theme() {
        assert_eq!(
            project_render_options(RenderOptions::new(DiffLayout::Split, DiffDensity::Full)),
            ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Split,
                density: ViewerDiffDensity::Full,
            }
        );
        assert_eq!(project_theme(Theme::Graphite), ViewerTheme::Graphite);
    }

    #[test]
    fn projection_preserves_wire_metadata_and_range_commit_shelf() {
        let view = view();
        let active = project_diff_view(
            &view,
            &view,
            identity(ViewerDiffDensity::Compact),
            ViewerCommitSelection::None,
        );

        assert_eq!(u64::from(active.identity.tab_id), 7);
        assert_eq!(active.files[0].id.as_str(), "file-0");
        assert_eq!(active.files[1].id.as_str(), "file-1");
        assert_eq!(active.files[0].anchor_id, "f-src-a-b-rs");
        assert!(active.files[0].can_open_in_editor);
        assert!(!active.files[1].can_open_in_editor);
        assert_eq!(
            active.commits[0]
                .id
                .abbreviated(CommitIdAbbreviation::TenCharacters),
            "0123456789"
        );
        assert!(active.commits[0].is_merge);
        assert_eq!(
            active
                .extension_filter
                .unwrap()
                .hidden_paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["Cargo.lock"]
        );
    }
}
