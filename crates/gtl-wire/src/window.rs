pub use gtl_models::settings::ViewerScalePercent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    Minimize,
    ToggleMaximize,
    Close,
}

/// Tray menu labels in the viewer's display language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayLabels {
    pub show: String,
    pub quit: String,
}

/// Opens the native folder picker with a title in the viewer's display language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickProjectFolder {
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub custom_titlebar: bool,
    pub maximized: bool,
    pub visible: bool,
}
