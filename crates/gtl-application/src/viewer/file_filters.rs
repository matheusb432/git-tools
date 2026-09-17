#[derive(Debug, thiserror::Error)]
pub enum FileFiltersError {
    #[error(transparent)]
    State(#[from] super::ViewerStateError),
    #[error(transparent)]
    Settings(#[from] crate::ports::UserSettingsLoadError),
    #[error(transparent)]
    Edit(#[from] crate::settings::edit_settings::EditSettingsError),
    #[error(transparent)]
    Diff(#[from] crate::diffs::set_diff_file_exclusions::SetDiffFileExclusionsError),
    #[error("the viewer changed")]
    Changed,
    #[error("the revealed files exceed the viewer cache limit")]
    TooLarge,
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}
