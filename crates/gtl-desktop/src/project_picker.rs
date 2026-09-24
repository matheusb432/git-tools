use gtl_client::ViewerClientError;
use tauri_plugin_dialog::DialogExt as _;

#[tauri::command]
pub(crate) async fn desktop_pick_project_folder(
    window: tauri::WebviewWindow,
) -> Result<Option<String>, ViewerClientError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_title("Choose a folder to scan")
        .pick_folder(move |folder| {
            let _ = sender.send(folder);
        });
    let picked = receiver
        .await
        .map_err(|_| ViewerClientError::desktop(&"the folder picker closed unexpectedly"))?;
    picked
        .map(|folder| {
            let path = folder
                .into_path()
                .map_err(|error| ViewerClientError::desktop(&error))?;
            let path = path
                .canonicalize()
                .map_err(|error| ViewerClientError::desktop(&error))?;
            path.to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| ViewerClientError::desktop(&"the selected folder path is not UTF-8"))
        })
        .transpose()
}
