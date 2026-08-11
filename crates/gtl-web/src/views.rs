pub(crate) mod diffs;
#[cfg(feature = "desktop")]
mod user_settings;

#[cfg(feature = "desktop")]
pub(crate) use diffs::{DiffHistoryView, DiffWorkspaceView};
#[cfg(feature = "desktop")]
pub(crate) use user_settings::UserSettingsView;
