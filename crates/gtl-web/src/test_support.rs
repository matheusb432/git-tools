use std::error::Error;

use gtl_models::{
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPage, HistoryPageCount, HistoryPageNumber, RenderHistoryId, ViewerRangeGeneration,
        ViewerSelectionGeneration, ViewerTabId,
    },
};
use gtl_wire::viewer::{
    ViewerActiveView, ViewerCodeLine, ViewerCodeSpan, ViewerCommandLine, ViewerCommitSelection,
    ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerRenderOptions,
    ViewerRowContentId, ViewerRowSourceState, ViewerUnifiedSourceRow, ViewerViewIdentity,
};

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub(crate) fn viewer_tab_id(value: u64) -> TestResult<ViewerTabId> {
    Ok(ViewerTabId::try_new(value)?)
}

pub(crate) fn render_history_id(value: i64) -> TestResult<RenderHistoryId> {
    Ok(RenderHistoryId::try_new(value)?)
}

pub(crate) fn viewer_active_view(tab_id: ViewerTabId) -> ViewerActiveView {
    ViewerActiveView {
        modified_files: false,
        source: gtl_wire::viewer::ViewerViewSource::Repository,
        identity: ViewerViewIdentity {
            tab_id,
            range_generation: ViewerRangeGeneration::new(1),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: gtl_wire::viewer::ViewerDiffLayout::Split,
                density: gtl_wire::viewer::ViewerDiffDensity::Full,
            },
        },
        content_id: ViewerRowContentId::from_digest([7; 32]),
        row_source: ViewerRowSourceState::Ready,
        title: gtl_models::diffs::DiffViewTitle::Diff,
        command: ViewerCommandLine {
            lead: "git diff ".to_owned(),
            range: "main..HEAD".to_owned(),
            trail: String::new(),
        },
        files: Vec::new(),
        commit_count: 0,
        commits: Vec::new(),
        commit_selection: ViewerCommitSelection::None,
        footer: ViewerFooter {
            command: "git diff main..HEAD".to_owned(),
        },
        extension_filter: None,
        changes_since: None,
    }
}

pub(crate) fn viewer_file_summary(
    index: usize,
    path: &str,
    status: ViewerFileStatus,
    added: u64,
    removed: u64,
) -> TestResult<ViewerFileSummary> {
    Ok(ViewerFileSummary {
        review: None,
        source_id: None,
        id: ViewerDiffFileId::for_index(index),
        path: repository_relative_path(path)?,
        absolute_path: Some(absolute_file_path(format!("/repo/{path}"))?),
        anchor_id: format!("f-{}", path.replace(['/', '.'], "-")),
        added: gtl_models::diffs::DiffLineCount::new(added),
        removed: gtl_models::diffs::DiffLineCount::new(removed),
        status,
        can_open_in_editor: status != ViewerFileStatus::Deleted,
        initially_expanded: true,
        row_count: 1,
    })
}

pub(crate) fn history_page_number(value: u32) -> TestResult<HistoryPageNumber> {
    Ok(HistoryPageNumber::try_new(value)?)
}

pub(crate) fn history_page(number: u32, count: u32) -> TestResult<HistoryPage> {
    Ok(HistoryPage::new(
        history_page_number(number)?,
        HistoryPageCount::try_new(count)?,
    )?)
}

pub(crate) fn project_name(value: &str) -> TestResult<ProjectName> {
    Ok(ProjectName::try_from(value)?)
}

pub(crate) fn recipe_label(name: &str) -> TestResult<gtl_models::recipes::RecipeLabel> {
    Ok(gtl_models::recipes::RecipeLabel::Named {
        name: project_name(name)?,
    })
}

pub(crate) fn repository_relative_path(value: &str) -> TestResult<RepositoryRelativePath> {
    Ok(RepositoryRelativePath::try_new(value.into())?)
}

pub(crate) fn absolute_file_path(
    value: impl Into<std::path::PathBuf>,
) -> TestResult<AbsoluteFilePath> {
    Ok(AbsoluteFilePath::try_new(value.into())?)
}

pub(crate) fn machine_timestamp(value: &str) -> TestResult<MachineTimestamp> {
    Ok(MachineTimestamp::try_from(value)?)
}

pub(crate) fn code_line(text: &str, omitted_character_count: Option<usize>) -> ViewerCodeLine {
    ViewerCodeLine {
        text: text.to_owned(),
        spans: vec![ViewerCodeSpan {
            byte_start: 0,
            byte_end: text.len(),
            syntax_class: None,
            changed: false,
        }],
        omitted_character_count,
    }
}

pub(crate) fn unified_source_row(
    text: &str,
    old_line_number: Option<u32>,
    new_line_number: Option<u32>,
    omitted_character_count: Option<usize>,
) -> ViewerUnifiedSourceRow {
    ViewerUnifiedSourceRow {
        old_line_number,
        new_line_number,
        code: code_line(text, omitted_character_count),
    }
}
