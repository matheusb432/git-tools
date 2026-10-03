use gtl_models::failure::{ErrorMeta, Failure, Resource};
use gtl_wire::viewer::OpenViewerDiffFile;

use super::{
    ViewerState,
    diff_view::file_by_id,
    source::{self, ViewerSourceError},
};
use crate::{
    diffs::open_diff_file_in_configured_editor::{self, OpenDiffFileInConfiguredEditorError},
    ports::{FileSystemClient, TextEditorClient, UserSettingsReader},
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum OpenViewerDiffFileError {
    #[error(transparent)]
    #[meta(transparent)]
    Source(#[from] ViewerSourceError),
    #[error("the file is not present in the current diff")]
    #[meta(failure = Failure::Gone { resource: Resource::DiffFile })]
    MissingFile,
    #[error(transparent)]
    #[meta(transparent)]
    Open(#[from] OpenDiffFileInConfiguredEditorError),
}

#[cqrsy::command]
pub fn execute(
    request: &OpenViewerDiffFile,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    file_system: &impl FileSystemClient,
    text_editor: &impl TextEditorClient,
) -> Result<(), OpenViewerDiffFileError> {
    let snapshot = source::current(request.identity, state, settings)?;
    let file =
        file_by_id(snapshot.view(), &request.file).ok_or(OpenViewerDiffFileError::MissingFile)?;
    open_diff_file_in_configured_editor::execute(
        &file.path,
        snapshot.view(),
        file_system,
        text_editor,
    )?;
    Ok(())
}
