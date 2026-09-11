pub(crate) mod diffs;
#[cfg(feature = "desktop")]
pub(crate) mod settings_recovery;
#[cfg(feature = "desktop")]
mod user_settings;
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(crate) mod viewer_menu;
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(crate) mod viewer_settings_form;

#[cfg(feature = "desktop")]
pub(crate) use diffs::{DiffWorkspaceView, SnapshotHistory};
#[cfg(feature = "desktop")]
pub(crate) use user_settings::UserSettingsView;

#[cfg(feature = "desktop")]
pub(crate) mod projects;
