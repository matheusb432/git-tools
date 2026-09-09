use gtl_wire::viewer::OpenViewerDiffFile;

use super::{
    ViewerState,
    source::{self, ViewerSourceError},
};
use crate::{
    diffs::open_diff_file_in_configured_editor::{self, OpenDiffFileInConfiguredEditorError},
    ports::{FileSystemClient, TextEditorClient, UserSettingsReader},
};

#[derive(Debug, thiserror::Error)]
pub enum OpenViewerDiffFileError {
    #[error(transparent)]
    Source(#[from] ViewerSourceError),
    #[error("the file is not present in the current diff")]
    MissingFile,
    #[error(transparent)]
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
    let file = snapshot
        .view()
        .files
        .iter()
        .enumerate()
        .find(|(index, _)| gtl_wire::viewer::ViewerDiffFileId::for_index(*index) == request.file)
        .map(|(_, file)| file)
        .ok_or(OpenViewerDiffFileError::MissingFile)?;
    open_diff_file_in_configured_editor::execute(
        &file.path,
        snapshot.view(),
        file_system,
        text_editor,
    )?;
    Ok(())
}
