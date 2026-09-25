//! Keeps the tray menu in the viewer's display language.

use tauri::{Wry, menu::MenuItem};

/// Tray menu items whose text follows the viewer's language.
pub(crate) struct TrayMenuItems {
    pub(crate) show: MenuItem<Wry>,
    pub(crate) quit: MenuItem<Wry>,
}

// Tauri's async command expansion contains `unreachable!`.
#[allow(clippy::unreachable)]
pub(crate) mod command {
    use gtl_client::ViewerClientError;
    use gtl_wire::window::TrayLabels;
    use tauri::State;

    use super::TrayMenuItems;

    /// Replaces the tray menu text with labels the viewer localized.
    #[tauri::command]
    pub(crate) async fn desktop_tray_labels(
        items: State<'_, TrayMenuItems>,
        request: TrayLabels,
    ) -> Result<(), ViewerClientError> {
        let TrayLabels { show, quit } = request;
        items
            .show
            .set_text(show)
            .map_err(|error| ViewerClientError::desktop(&error))?;
        items
            .quit
            .set_text(quit)
            .map_err(|error| ViewerClientError::desktop(&error))
    }
}
