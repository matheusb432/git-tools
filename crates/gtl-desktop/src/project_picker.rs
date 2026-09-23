use tauri_plugin_dialog::DialogExt as _;

#[tauri::command]
pub(crate) async fn desktop_pick_project_folder(
    window: tauri::WebviewWindow,
) -> Result<Option<String>, String> {
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
        .map_err(|_| "folder picker closed unexpectedly".to_owned())?;
    picked
        .map(|folder| {
            let path = folder.into_path().map_err(|error| error.to_string())?;
            let path = path.canonicalize().map_err(|error| error.to_string())?;
            path.to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| "selected folder path is not UTF-8".to_owned())
        })
        .transpose()
}
