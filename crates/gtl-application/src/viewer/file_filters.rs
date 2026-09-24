use gtl_models::failure::{ErrorMeta, Failure, ViewerFailure};
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum FileFiltersError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] super::ViewerStateError),
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] crate::ports::UserSettingsLoadError),
    #[error(transparent)]
    #[meta(transparent)]
    Edit(#[from] crate::settings::edit_settings::EditSettingsError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Diff(#[from] crate::diffs::set_diff_file_exclusions::SetDiffFileExclusionsError),
    #[error("the viewer changed")]
    #[meta(failure = Failure::Changed)]
    Changed,
    #[error("the revealed files exceed the viewer cache limit")]
    #[meta(failure = ViewerFailure::RevealTooLarge)]
    TooLarge,
    #[error(transparent)]
    #[meta(private(Internal))]
    Database(#[from] anyhow::Error),
}
