pub(crate) mod diffs;
pub(crate) mod keybindings;
pub(crate) mod settings_recovery;
mod user_settings;
pub(crate) mod viewer_settings_button;
pub(crate) mod viewer_settings_form;

pub(crate) use diffs::{DiffWorkspaceView, SnapshotHistory};
pub(crate) use user_settings::UserSettingsView;

pub(crate) mod projects;

pub(crate) mod push;

pub(crate) mod commit_search;
