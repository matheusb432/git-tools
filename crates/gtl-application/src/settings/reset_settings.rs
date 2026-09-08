use std::path::PathBuf;

use crate::{
    ports::{Clock, UserSettingsEditError, UserSettingsRecovery},
    viewer::{ViewerState, ViewerStateError},
};

#[derive(Debug, thiserror::Error)]
pub enum ResetSettingsError {
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
    Clock(#[from] gtl_models::timestamps::TimestampError),
    #[error(transparent)]
    Viewer(#[from] ViewerStateError),
}

#[cqrsy::command]
pub fn execute(
    revision: &str,
    settings: &mut impl UserSettingsRecovery,
    clock: &impl Clock,
    viewer: &ViewerState,
) -> Result<PathBuf, ResetSettingsError> {
    let backup = settings.reset_invalid(revision, &clock.now()?)?;
    viewer.mark_shell_changed()?;
    Ok(backup)
}
