pub(crate) mod diffs;
#[cfg(feature = "desktop")]
mod user_settings;
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(crate) mod viewer_menu;
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(crate) mod viewer_settings_form;

#[cfg(feature = "desktop")]
pub(crate) use diffs::{DiffHistoryView, DiffWorkspaceView};
#[cfg(feature = "desktop")]
pub(crate) use user_settings::UserSettingsView;
