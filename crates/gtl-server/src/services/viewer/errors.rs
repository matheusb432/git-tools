use gtl_application::{
    diffs::open_diff_file_in_configured_editor::OpenDiffFileInConfiguredEditorError,
    viewer::{
        self, close_viewer_tabs::CloseViewerTabsError, find_viewer_diff::FindViewerDiffError,
        get_viewer_shell::GetViewerShellError, move_viewer_tab::MoveViewerTabError,
        open_viewer_diff_file::OpenViewerDiffFileError,
        read_viewer_diff_text::ReadViewerDiffTextError, source::ViewerSourceError,
    },
};
use tonic::Status;

use super::super::{unexpected, user_settings_load_error};

pub(super) fn source(error: ViewerSourceError) -> Status {
    match error {
        ViewerSourceError::Settings(error) => user_settings_load_error(error),
        ViewerSourceError::State(error) => viewer_state_error(error, "load viewer source"),
        ViewerSourceError::Changed => Status::aborted("viewer identity changed"),
        ViewerSourceError::Preparing => {
            Status::failed_precondition("the diff source is still being prepared")
        }
    }
}

pub(super) fn get_shell(error: GetViewerShellError) -> Status {
    match error {
        GetViewerShellError::Settings(error) => user_settings_load_error(error),
        GetViewerShellError::State(error) => viewer_state_error(error, "load viewer shell"),
        GetViewerShellError::Projection(error) => unexpected_viewer(error, "project viewer shell"),
        GetViewerShellError::CommitReload(error) => {
            unexpected_viewer(error, "restore selected viewer commit")
        }
    }
}

pub(super) fn read_diff_text(error: ReadViewerDiffTextError) -> Status {
    match error {
        ReadViewerDiffTextError::Source(error) => source(error),
        ReadViewerDiffTextError::Unavailable => {
            Status::not_found("viewer source range is unavailable")
        }
        ReadViewerDiffTextError::TooLarge => {
            Status::resource_exhausted("viewer source range exceeds the response limit")
        }
    }
}

pub(super) fn find_diff(error: FindViewerDiffError) -> Status {
    match error {
        FindViewerDiffError::Source(error) => source(error),
        FindViewerDiffError::Cancelled => Status::cancelled("viewer search was superseded"),
        FindViewerDiffError::RowIndexExhausted | FindViewerDiffError::MatchCountExhausted => {
            Status::resource_exhausted("viewer diff search exceeds server limits")
        }
    }
}

pub(super) fn open_diff_file(error: OpenViewerDiffFileError) -> Status {
    match error {
        OpenViewerDiffFileError::Source(error) => source(error),
        OpenViewerDiffFileError::MissingFile => {
            Status::not_found("viewer diff file is unavailable")
        }
        OpenViewerDiffFileError::Open(error) => open_file_error(error),
    }
}

pub(super) fn map_reserve_recipe(
    error: viewer::work::ReserveRecipeError,
    operation: &'static str,
) -> Status {
    match error {
        viewer::work::ReserveRecipeError::UnknownTab => {
            Status::not_found("viewer tab is not available")
        }
        error => unexpected_viewer(error, operation),
    }
}

pub(super) fn move_viewer_tab_error(error: MoveViewerTabError) -> Status {
    match error {
        MoveViewerTabError::UnknownTab => Status::not_found("viewer tab is not available"),
        MoveViewerTabError::State(error) => viewer_state_error(error, "move viewer tab"),
    }
}

pub(super) fn close_viewer_tabs_error(error: CloseViewerTabsError) -> Status {
    match error {
        CloseViewerTabsError::UnknownTab => Status::not_found("viewer tab is not available"),
        CloseViewerTabsError::ReserveWork(error) => {
            map_reserve_recipe(error, "refresh viewer after closing tabs")
        }
        error => unexpected_viewer(error, "close viewer tabs"),
    }
}

pub(super) fn map_reserve_commit(error: viewer::work::ReserveCommitError) -> Status {
    match error {
        viewer::work::ReserveCommitError::Selection(
            viewer::session::BeginCommitSelectionError::UnknownTab
            | viewer::session::BeginCommitSelectionError::UnknownCommit,
        ) => Status::not_found("viewer commit is not available"),
        viewer::work::ReserveCommitError::Selection(
            viewer::session::BeginCommitSelectionError::StaleRange
            | viewer::session::BeginCommitSelectionError::SelectionPending,
        ) => Status::aborted("viewer selection changed"),
        error => unexpected_viewer(error, "select viewer commit"),
    }
}

pub(super) fn open_file_error(error: OpenDiffFileInConfiguredEditorError) -> Status {
    match error {
        OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff
        | OpenDiffFileInConfiguredEditorError::DiffFileDeleted
        | OpenDiffFileInConfiguredEditorError::DiffFileUnavailable => {
            Status::failed_precondition(error.to_string())
        }
        OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository => {
            Status::permission_denied("diff file resolves outside its repository")
        }
        error => unexpected(error, "open viewer diff file"),
    }
}

pub(super) fn viewer_state_error(
    error: viewer::ViewerStateError,
    operation: &'static str,
) -> Status {
    unexpected_viewer(error, operation)
}

pub(super) fn unexpected_viewer(
    error: impl std::fmt::Debug + std::fmt::Display,
    operation: &'static str,
) -> Status {
    tracing::error!(error = ?error, operation, "viewer operation failed");
    Status::internal("viewer operation failed")
}
