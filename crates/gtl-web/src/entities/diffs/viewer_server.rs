use gtl_wire::viewer::{
    EditSettingsRequest, FindViewerDiff, GetViewerHistoryCopy, ListViewerCommits,
    ListViewerHistory, MoveViewerTab, OpenViewerDiffFile, OpenViewerHistory, RenameViewerSnapshot,
    SearchViewerFiles, SelectViewerCommit, SetViewerModifiedFiles, SetViewerTabLive,
    SetViewerTabPinned, StreamViewerRows, ViewerCommitPage, ViewerDiffSearchResult,
    ViewerFileSearchResult, ViewerHistoryCopyPayload, ViewerHistoryPage, ViewerShell,
    ViewerStateChanged, ViewerTabRequest, ViewerUserSettings,
    projects::{
        DiscoverProjectRepositories, ImportProjectRepositories, ListViewerProjects,
        OpenUnpushedProjectDiffsOk, OpenViewerProject, OpenViewerProjectOk, ProjectDiscovery,
        ProjectImportResult, SetViewerProjectStatus, UpdateViewerProject, ViewerProjectPage,
    },
};

use crate::shared::viewer_client::ViewerClientError;
#[cfg(target_arch = "wasm32")]
use crate::shared::viewer_client::connect_viewer;

macro_rules! viewer_request {
    ($name:ident, $request:ty, $response:ty, $method:ident) => {
        #[cfg(target_arch = "wasm32")]
        pub(crate) async fn $name(request: $request) -> Result<$response, ViewerClientError> {
            let mut viewer = connect_viewer().await?;
            viewer.client.$method(request).await
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub(crate) fn $name(
            _request: $request,
        ) -> impl std::future::Future<Output = Result<$response, ViewerClientError>> {
            std::future::ready(Err(ViewerClientError::Disconnected))
        }
    };
}

macro_rules! viewer_query {
    ($name:ident, $response:ty, $method:ident) => {
        #[cfg(target_arch = "wasm32")]
        pub(crate) async fn $name() -> Result<$response, ViewerClientError> {
            let mut viewer = connect_viewer().await?;
            viewer.client.$method().await
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub(crate) fn $name()
        -> impl std::future::Future<Output = Result<$response, ViewerClientError>> {
            std::future::ready(Err(ViewerClientError::Disconnected))
        }
    };
}

viewer_query!(get_shell, ViewerShell, get_shell);
viewer_request!(
    set_diff_file_reviewed,
    gtl_wire::diff_review::SetDiffFileReviewed,
    (),
    set_diff_file_reviewed
);
viewer_request!(
    list_projects,
    ListViewerProjects,
    ViewerProjectPage,
    list_projects
);
viewer_request!(
    discover_project_repositories,
    DiscoverProjectRepositories,
    ProjectDiscovery,
    discover_project_repositories
);
viewer_request!(
    import_project_repositories,
    ImportProjectRepositories,
    Vec<ProjectImportResult>,
    import_project_repositories
);

viewer_request!(
    open_project,
    OpenViewerProject,
    OpenViewerProjectOk,
    open_project
);
viewer_query!(
    open_unpushed_project_diffs,
    OpenUnpushedProjectDiffsOk,
    open_unpushed_project_diffs
);
viewer_request!(
    stream_rows,
    StreamViewerRows,
    gtl_client::ViewerRowStream,
    stream_rows
);
viewer_request!(
    list_commits,
    ListViewerCommits,
    ViewerCommitPage,
    list_commits
);
viewer_request!(
    search_files,
    SearchViewerFiles,
    ViewerFileSearchResult,
    search_files
);
viewer_request!(find_diff, FindViewerDiff, ViewerDiffSearchResult, find_diff);
viewer_request!(
    read_diff_text,
    gtl_wire::viewer::ReadViewerDiffText,
    Vec<gtl_wire::viewer::ViewerDiffTextLine>,
    read_diff_text
);
viewer_request!(
    list_history,
    ListViewerHistory,
    ViewerHistoryPage,
    list_history
);
viewer_request!(open_history, OpenViewerHistory, ViewerShell, open_history);
viewer_request!(
    get_history_copy,
    GetViewerHistoryCopy,
    ViewerHistoryCopyPayload,
    get_history_copy
);
viewer_request!(activate_tab, ViewerTabRequest, ViewerShell, activate_tab);
viewer_request!(move_tab, MoveViewerTab, ViewerShell, move_tab);
viewer_request!(close_tab, ViewerTabRequest, ViewerShell, close_tab);
viewer_request!(refresh_tab, ViewerTabRequest, ViewerShell, refresh_tab);
viewer_request!(
    set_changes_since,
    gtl_wire::viewer::SetViewerChangesSince,
    ViewerShell,
    set_changes_since
);
viewer_request!(update_tab, ViewerTabRequest, ViewerShell, update_tab);
viewer_request!(set_tab_live, SetViewerTabLive, ViewerShell, set_tab_live);
viewer_request!(
    select_commit,
    SelectViewerCommit,
    ViewerShell,
    select_commit
);
viewer_request!(
    clear_commit_selection,
    ViewerTabRequest,
    ViewerShell,
    clear_commit_selection
);
viewer_request!(open_diff_file, OpenViewerDiffFile, (), open_diff_file);
viewer_query!(
    get_settings_recovery,
    gtl_wire::viewer::ViewerSettingsRecovery,
    get_settings_recovery
);
viewer_request!(
    reset_settings,
    gtl_wire::viewer::ResetSettings,
    gtl_wire::viewer::ResetSettingsOk,
    reset_settings
);
viewer_query!(get_settings, ViewerUserSettings, get_settings);
viewer_request!(edit_settings, EditSettingsRequest, (), edit_settings);

#[cfg(target_arch = "wasm32")]
pub(crate) async fn listen_for_state_changes<Ready, Handler>(
    request: gtl_wire::viewer::WatchViewer,
    on_ready: Ready,
    on_event: Handler,
) -> Result<(), ViewerClientError>
where
    Ready: Fn(String) + 'static,
    Handler: Fn(ViewerStateChanged) + 'static,
{
    let mut viewer = connect_viewer().await?;
    let server_instance_id = viewer.server_instance_id;
    let mut stream = viewer.client.watch(request).await?;
    on_ready(server_instance_id);
    while let Some(event) = stream.message().await? {
        on_event(event);
    }
    Err(ViewerClientError::Disconnected)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn listen_for_state_changes<Ready, Handler>(
    _request: gtl_wire::viewer::WatchViewer,
    _on_ready: Ready,
    _on_event: Handler,
) -> impl std::future::Future<Output = Result<(), ViewerClientError>>
where
    Ready: Fn(String) + 'static,
    Handler: Fn(ViewerStateChanged) + 'static,
{
    std::future::ready(Err(ViewerClientError::Disconnected))
}

viewer_request!(update_project, UpdateViewerProject, (), update_project);
viewer_request!(
    set_project_status,
    SetViewerProjectStatus,
    (),
    set_project_status
);

viewer_request!(
    set_modified_files,
    SetViewerModifiedFiles,
    (),
    set_modified_files
);

viewer_request!(set_tab_pinned, SetViewerTabPinned, (), set_tab_pinned);
viewer_request!(close_other_tabs, ViewerTabRequest, (), close_other_tabs);

viewer_request!(
    get_file_filters,
    gtl_wire::viewer::ViewerTabRequest,
    gtl_wire::viewer::file_filters::ViewerFileFilters,
    get_file_filters
);
viewer_request!(
    set_file_filters,
    gtl_wire::viewer::file_filters::SetViewerFileFilters,
    (),
    set_file_filters
);

viewer_request!(
    create_push,
    gtl_wire::viewer::push::CreateViewerPush,
    gtl_wire::viewer::push::ViewerPushRequest,
    create_push
);
viewer_request!(
    get_push,
    gtl_wire::viewer::push::ViewerPushRequest,
    gtl_wire::viewer::push::ViewerPushStatus,
    get_push
);
viewer_request!(
    start_push,
    gtl_wire::viewer::push::ViewerPushRequest,
    (),
    start_push
);

viewer_request!(
    get_push_availability,
    gtl_wire::viewer::ViewerViewIdentity,
    gtl_wire::viewer::push::ViewerPushState,
    get_push_availability
);

viewer_request!(rename_snapshot, RenameViewerSnapshot, (), rename_snapshot);

viewer_request!(
    search_commits,
    gtl_wire::viewer::commit_search::SearchViewerCommits,
    gtl_wire::viewer::commit_search::ViewerCommitSearchResult,
    search_commits
);
viewer_request!(
    open_commit,
    gtl_wire::viewer::commit_search::OpenViewerCommit,
    OpenViewerProjectOk,
    open_commit
);
