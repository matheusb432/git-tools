use std::error::Error;

use gtl_models::{
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{HistoryPage, HistoryPageCount, HistoryPageNumber, RenderHistoryId, ViewerTabId},
};

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub(crate) fn viewer_tab_id(value: u64) -> TestResult<ViewerTabId> {
    Ok(ViewerTabId::try_new(value)?)
}

pub(crate) fn render_history_id(value: i64) -> TestResult<RenderHistoryId> {
    Ok(RenderHistoryId::try_new(value)?)
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
