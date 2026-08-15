use gtl_wire::viewer::{
    LoadViewerDiffLines, VIEWER_DIFF_LINES_PAGE_MAX_BYTES, ViewerActiveView, ViewerApiError,
    ViewerAppliedExclusions, ViewerCommandLine, ViewerCommitSelection, ViewerCommitSummary,
    ViewerDiffCursor, ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLines, ViewerFileStatus,
    ViewerFileSummary, ViewerFooter, ViewerRenderOptions, ViewerResource, ViewerTheme,
    ViewerViewIdentity,
};

use crate::{
    diffs::{FileDiff, FileStatus, View},
    viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
};

const GIANT_FILE_CHARACTERS: usize = 250_000;

/// Projects validated application rendering options into the shared client contract.
#[must_use]
pub const fn project_render_options(options: RenderOptions) -> ViewerRenderOptions {
    ViewerRenderOptions {
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
        Theme::Light => ViewerTheme::Light,
        Theme::Hearth => ViewerTheme::Hearth,
        Theme::Mirage => ViewerTheme::Mirage,
        Theme::Glacier => ViewerTheme::Glacier,
        Theme::Noir => ViewerTheme::Noir,
        Theme::Graphite => ViewerTheme::Graphite,
    }
}

/// Returns the stable document anchor for a file in a rendered diff view.
#[must_use]
pub fn diff_file_anchor_id(path: &str) -> String {
    let mut body = String::new();
    let mut last_was_separator = false;

    for character in path.chars() {
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
    ViewerActiveView {
        identity,
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
            .map(|(index, file)| project_file(view, identity, index, file))
            .collect(),
        commits_label: range_view.commits_label.clone(),
        commits: range_view
            .commits
            .iter()
            .map(|commit| ViewerCommitSummary {
                id: commit.id.clone(),
                subject: commit.subject.clone(),
                body: commit.body.clone(),
                date: commit.date.clone(),
                iso: commit.iso.clone(),
                is_merge: commit.is_merge(),
            })
            .collect(),
        commit_selection,
        footer: ViewerFooter {
            command: view.foot.cmd.clone(),
        },
        exclusions: view
            .exclusions
            .as_ref()
            .map(|exclusions| ViewerAppliedExclusions {
                extensions: exclusions.extensions.clone(),
                hidden_paths: exclusions.hidden_paths.clone(),
            }),
    }
}

/// Projects one identity-bound, byte-bounded page of raw diff lines.
///
/// One oversized line is returned by itself so every valid cursor can advance.
pub fn project_diff_lines(
    view: &View,
    request: &LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ViewerApiError> {
    let file = file_by_id(view, &request.file).ok_or(ViewerApiError::NotFound {
        resource: ViewerResource::DiffFile,
    })?;
    let lines = selected_lines(file, request.identity.render_options.density);
    let start =
        usize::try_from(request.cursor.position()).map_err(|_| ViewerApiError::InvalidRequest)?;
    if start > lines.len() {
        return Err(ViewerApiError::InvalidRequest);
    }

    let mut page = Vec::new();
    let mut bytes = 0_usize;
    for line in &lines[start..] {
        let crosses_bound =
            !page.is_empty() && bytes.saturating_add(line.len()) > VIEWER_DIFF_LINES_PAGE_MAX_BYTES;
        if crosses_bound {
            break;
        }
        bytes = bytes.saturating_add(line.len());
        page.push(line.clone());
    }

    let end = start
        .checked_add(page.len())
        .ok_or(ViewerApiError::InvalidRequest)?;
    let next = (end < lines.len())
        .then(|| u32::try_from(end).map(ViewerDiffCursor::new))
        .transpose()
        .map_err(|_| ViewerApiError::InvalidRequest)?;
    Ok(ViewerDiffLines {
        identity: request.identity,
        file: request.file.clone(),
        cursor: request.cursor,
        lines: page,
        next,
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
        id: ViewerDiffFileId::for_index(index),
        path: file.path.clone(),
        absolute_path: format!("{}/{}", view.repo_root, file.path),
        anchor_id: diff_file_anchor_id(&file.path),
        added: file.added,
        removed: file.removed,
        status: viewer_file_status(status),
        can_open_in_editor: status != FileStatus::Deleted,
        initially_expanded: selected_lines(file, identity.render_options.density)
            .iter()
            .map(String::len)
            .sum::<usize>()
            <= GIANT_FILE_CHARACTERS,
    }
}

fn file_by_id<'view>(view: &'view View, id: &ViewerDiffFileId) -> Option<&'view FileDiff> {
    view.files.iter().enumerate().find_map(|(index, file)| {
        (ViewerDiffFileId::for_index(index).as_str() == id.as_str()).then_some(file)
    })
}

fn selected_lines(file: &FileDiff, density: ViewerDiffDensity) -> &[String] {
    match (density, file.full_lines.as_deref()) {
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
    use gtl_models::diffs::{AppliedExclusions, CommitIdAbbreviation};
    use gtl_wire::viewer::{
        LoadViewerDiffLines, VIEWER_DIFF_LINES_PAGE_MAX_BYTES, ViewerApiError,
        ViewerCommitSelection, ViewerDiffCursor, ViewerDiffDensity, ViewerDiffFileId,
        ViewerDiffLayout, ViewerRenderOptions, ViewerResource, ViewerTheme, ViewerViewIdentity,
    };

    use super::{
        diff_file_anchor_id, project_diff_lines, project_diff_view, project_render_options,
        project_theme,
    };
    use crate::{
        diffs::{Cmd, FileDiff, View},
        testing,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    fn identity(density: ViewerDiffDensity) -> ViewerViewIdentity {
        ViewerViewIdentity {
            tab_id: 7,
            range_generation: 8,
            selection_generation: 9,
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density,
            },
        }
    }

    fn view() -> View {
        View {
            repo_name: "git-tools".into(),
            commits: vec![testing::diffs::commit_with(
                "0123456789abcdef0123456789abcdef01234567",
                "subject",
                &[
                    "1111111111111111111111111111111111111111",
                    "2222222222222222222222222222222222222222",
                ],
            )],
            files: vec![
                FileDiff {
                    path: "src/a b.rs".into(),
                    added: 2,
                    removed: 1,
                    lines: vec!["new file mode 100644".into(), "+compact".into()],
                    full_lines: Some(vec!["new file mode 100644".into(), "+full".into()]),
                },
                FileDiff {
                    path: "removed.rs".into(),
                    added: 0,
                    removed: 1,
                    lines: vec!["deleted file mode 100644".into()],
                    full_lines: None,
                },
            ],
            title: "Feature diff".into(),
            cmd: Cmd {
                lead: "git diff ".into(),
                range: "main...feature".into(),
                trail: " --".into(),
            },
            commits_label: "2 commits".into(),
            exclusions: Some(AppliedExclusions {
                extensions: vec!["lock".into()],
                hidden_paths: vec!["Cargo.lock".into()],
            }),
            ..testing::diffs::view()
        }
    }

    #[test]
    fn file_anchor_normalizes_paths_and_keeps_a_stable_prefix() {
        assert_eq!(diff_file_anchor_id("src/a b.rs"), "f-src-a-b-rs");
        assert_eq!(diff_file_anchor_id("---"), "f-");
    }

    #[test]
    fn shared_contract_projection_covers_every_render_option_and_theme() {
        assert_eq!(
            project_render_options(RenderOptions::new(DiffLayout::Split, DiffDensity::Full)),
            ViewerRenderOptions {
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

        assert_eq!(active.identity.tab_id, 7);
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
            active.exclusions.expect("applied exclusions").hidden_paths,
            ["Cargo.lock"]
        );
    }

    #[test]
    fn line_pages_select_density_and_bound_progress_by_decoded_bytes() {
        let mut view = view();
        view.files[0].lines = vec![
            "x".repeat(VIEWER_DIFF_LINES_PAGE_MAX_BYTES),
            "compact-tail".into(),
        ];
        let compact_request = LoadViewerDiffLines {
            identity: identity(ViewerDiffDensity::Compact),
            file: ViewerDiffFileId::for_index(0),
            cursor: ViewerDiffCursor::START,
        };

        let first = project_diff_lines(&view, &compact_request).expect("first compact page");
        assert_eq!(first.lines.len(), 1);
        assert_eq!(first.next, Some(ViewerDiffCursor::new(1)));

        let tail = project_diff_lines(
            &view,
            &LoadViewerDiffLines {
                cursor: first.next.expect("compact continuation"),
                ..compact_request
            },
        )
        .expect("compact tail page");
        assert_eq!(tail.lines, ["compact-tail"]);
        assert_eq!(tail.next, None);

        let full = project_diff_lines(
            &view,
            &LoadViewerDiffLines {
                identity: identity(ViewerDiffDensity::Full),
                file: ViewerDiffFileId::for_index(0),
                cursor: ViewerDiffCursor::START,
            },
        )
        .expect("full page");
        assert_eq!(full.lines, ["new file mode 100644", "+full"]);

        let unknown = project_diff_lines(
            &view,
            &LoadViewerDiffLines {
                identity: identity(ViewerDiffDensity::Compact),
                file: ViewerDiffFileId::for_index(99),
                cursor: ViewerDiffCursor::START,
            },
        );
        assert_eq!(
            unknown,
            Err(ViewerApiError::NotFound {
                resource: ViewerResource::DiffFile,
            })
        );

        let out_of_range = project_diff_lines(
            &view,
            &LoadViewerDiffLines {
                identity: identity(ViewerDiffDensity::Compact),
                file: ViewerDiffFileId::for_index(0),
                cursor: ViewerDiffCursor::new(3),
            },
        );
        assert_eq!(out_of_range, Err(ViewerApiError::InvalidRequest));
    }
}
