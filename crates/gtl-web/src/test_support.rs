use std::error::Error;

#[cfg(feature = "desktop")]
use gtl_models::viewer::{HistoryPage, HistoryPageCount, HistoryPageNumber, RenderHistoryId};
use gtl_models::{
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::ViewerTabId,
};
use gtl_wire::viewer::{ViewerCodeLine, ViewerCodeSpan, ViewerUnifiedSourceRow};

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub(crate) fn viewer_tab_id(value: u64) -> TestResult<ViewerTabId> {
    Ok(ViewerTabId::try_new(value)?)
}

#[cfg(feature = "desktop")]
pub(crate) fn render_history_id(value: i64) -> TestResult<RenderHistoryId> {
    Ok(RenderHistoryId::try_new(value)?)
}

#[cfg(feature = "desktop")]
pub(crate) fn history_page_number(value: u32) -> TestResult<HistoryPageNumber> {
    Ok(HistoryPageNumber::try_new(value)?)
}

#[cfg(feature = "desktop")]
pub(crate) fn history_page(number: u32, count: u32) -> TestResult<HistoryPage> {
    Ok(HistoryPage::new(
        history_page_number(number)?,
        HistoryPageCount::try_new(count)?,
    )?)
}

pub(crate) fn project_name(value: &str) -> TestResult<ProjectName> {
    Ok(ProjectName::try_from(value)?)
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

pub(crate) fn code_line(text: &str, long_line_character_count: Option<usize>) -> ViewerCodeLine {
    ViewerCodeLine {
        text: text.to_owned(),
        spans: vec![ViewerCodeSpan {
            text: text.to_owned(),
            syntax_class: None,
            changed: false,
        }],
        long_line_character_count,
    }
}

pub(crate) fn unified_source_row(
    text: &str,
    old_line_number: Option<u32>,
    new_line_number: Option<u32>,
    long_line_character_count: Option<usize>,
) -> ViewerUnifiedSourceRow {
    ViewerUnifiedSourceRow {
        old_line_number,
        new_line_number,
        code: code_line(text, long_line_character_count),
    }
}
