use gtl_client::GtlClient;
use gtl_wire::v1;

pub(crate) struct ServerClient {
    runtime: tokio::runtime::Runtime,
    client: GtlClient,
}

pub(crate) struct ServerStatus {
    pub(crate) endpoint: std::path::PathBuf,
    pub(crate) instance_id: String,
}

impl ServerClient {
    pub(crate) fn connect() -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let client = runtime.block_on(GtlClient::connect_local())?;
        Ok(Self { runtime, client })
    }

    pub(crate) fn status(&self) -> anyhow::Result<ServerStatus> {
        let server_info = self
            .runtime
            .block_on(self.client.get_viewer_server_info())?;
        Ok(ServerStatus {
            endpoint: self.client.endpoint().path().to_path_buf(),
            instance_id: server_info.server_instance_id().to_owned(),
        })
    }

    pub(crate) fn render_diff(
        &self,
        request: v1::RenderDiffRequest,
    ) -> anyhow::Result<v1::RenderDiffResponse> {
        Ok(self.runtime.block_on(self.client.render_diff(request))?)
    }

    pub(crate) fn present_diff(
        &self,
        request: v1::PresentDiffRequest,
    ) -> anyhow::Result<v1::PresentDiffResponse> {
        Ok(self.runtime.block_on(self.client.present_diff(request))?)
    }

    pub(crate) fn present_merge_diff(
        &self,
        request: v1::PresentMergeDiffRequest,
    ) -> anyhow::Result<v1::PresentMergeDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.present_merge_diff(request))?)
    }

    pub(crate) fn present_subrepository_diffs(
        &self,
        request: v1::PresentSubrepositoryDiffsRequest,
    ) -> anyhow::Result<v1::PresentSubrepositoryDiffsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.present_subrepository_diffs(request))?)
    }

    pub(crate) fn present_project_repository_diffs(
        &self,
        request: v1::PresentProjectRepositoryDiffsRequest,
    ) -> anyhow::Result<v1::PresentProjectRepositoryDiffsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.present_project_repository_diffs(request))?)
    }

    pub(crate) fn render_merge_diff(
        &self,
        request: v1::RenderMergeDiffRequest,
    ) -> anyhow::Result<v1::RenderMergeDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.render_merge_diff(request))?)
    }

    pub(crate) fn render_subrepository_diffs(
        &self,
        request: v1::RenderSubrepositoryDiffsRequest,
    ) -> anyhow::Result<v1::RenderSubrepositoryDiffsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.render_subrepository_diffs(request))?)
    }

    pub(crate) fn render_project_repository_diffs(
        &self,
        request: v1::RenderProjectRepositoryDiffsRequest,
    ) -> anyhow::Result<v1::RenderProjectRepositoryDiffsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.render_project_repository_diffs(request))?)
    }

    pub(crate) fn push_project_repositories(
        &self,
        request: v1::PushProjectRepositoriesRequest,
    ) -> anyhow::Result<v1::PushProjectRepositoriesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.push_project_repositories(request))?)
    }

    pub(crate) fn pull_project_repositories(
        &self,
        request: v1::PullProjectRepositoriesRequest,
    ) -> anyhow::Result<v1::PullProjectRepositoriesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.pull_project_repositories(request))?)
    }

    pub(crate) fn get_project_repository_statuses(
        &self,
    ) -> anyhow::Result<v1::GetProjectRepositoryStatusesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_project_repository_statuses())?)
    }

    pub(crate) fn plan_repository_push(
        &self,
        request: v1::PlanRepositoryPushRequest,
    ) -> anyhow::Result<v1::PlanRepositoryPushResponse> {
        Ok(self
            .runtime
            .block_on(self.client.plan_repository_push(request))?)
    }

    pub(crate) fn execute_repository_push(
        &self,
        request: v1::ExecuteRepositoryPushRequest,
    ) -> anyhow::Result<v1::ExecuteRepositoryPushResponse> {
        Ok(self
            .runtime
            .block_on(self.client.execute_repository_push(request))?)
    }

    pub(crate) fn plan_recursive_repository_push(
        &self,
        request: v1::PlanRecursiveRepositoryPushRequest,
    ) -> anyhow::Result<v1::PlanRecursiveRepositoryPushResponse> {
        Ok(self
            .runtime
            .block_on(self.client.plan_recursive_repository_push(request))?)
    }

    pub(crate) fn execute_recursive_repository_push(
        &self,
        request: v1::ExecuteRecursiveRepositoryPushRequest,
    ) -> anyhow::Result<v1::ExecuteRecursiveRepositoryPushResponse> {
        Ok(self
            .runtime
            .block_on(self.client.execute_recursive_repository_push(request))?)
    }

    pub(crate) fn get_project_repository(
        &self,
        request: v1::GetProjectRepositoryRequest,
    ) -> anyhow::Result<v1::GetProjectRepositoryResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_project_repository(request))?)
    }

    pub(crate) fn pause_project(
        &self,
        request: v1::PauseProjectRequest,
    ) -> anyhow::Result<v1::PauseProjectResponse> {
        Ok(self.runtime.block_on(self.client.pause_project(request))?)
    }

    pub(crate) fn resume_project(
        &self,
        request: v1::ResumeProjectRequest,
    ) -> anyhow::Result<v1::ResumeProjectResponse> {
        Ok(self.runtime.block_on(self.client.resume_project(request))?)
    }

    pub(crate) fn pull_repository(
        &self,
        request: v1::PullRepositoryRequest,
    ) -> anyhow::Result<v1::PullRepositoryResponse> {
        Ok(self
            .runtime
            .block_on(self.client.pull_repository(request))?)
    }

    pub(crate) fn get_repository_status(
        &self,
        request: v1::GetRepositoryStatusRequest,
    ) -> anyhow::Result<v1::GetRepositoryStatusResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_repository_status(request))?)
    }

    pub(crate) fn get_recursive_repository_statuses(
        &self,
        request: v1::GetRecursiveRepositoryStatusesRequest,
    ) -> anyhow::Result<v1::GetRecursiveRepositoryStatusesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_recursive_repository_statuses(request))?)
    }

    pub(crate) fn get_push_confirmation_requirement(
        &self,
    ) -> anyhow::Result<v1::GetPushConfirmationRequirementResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_push_confirmation_requirement())?)
    }

    pub(crate) fn set_viewer_theme(
        &self,
        request: v1::SetViewerThemeRequest,
    ) -> anyhow::Result<v1::SetViewerThemeResponse> {
        Ok(self
            .runtime
            .block_on(self.client.set_viewer_theme(request))?)
    }

    pub(crate) fn save_and_present_live_view(
        &self,
        request: v1::SaveAndPresentLiveViewRequest,
    ) -> anyhow::Result<v1::SaveAndPresentLiveViewResponse> {
        Ok(self
            .runtime
            .block_on(self.client.save_and_present_live_view(request))?)
    }

    pub(crate) fn save_and_present_project_live_views(
        &self,
        request: v1::SaveAndPresentProjectLiveViewsRequest,
    ) -> anyhow::Result<v1::SaveAndPresentProjectLiveViewsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.save_and_present_project_live_views(request))?)
    }

    pub(crate) fn plan_tag_bump(
        &self,
        request: v1::PlanTagBumpRequest,
    ) -> anyhow::Result<v1::PlanTagBumpResponse> {
        Ok(self.runtime.block_on(self.client.plan_tag_bump(request))?)
    }

    pub(crate) fn execute_tag_bump(
        &self,
        request: v1::ExecuteTagBumpRequest,
    ) -> anyhow::Result<v1::ExecuteTagBumpResponse> {
        Ok(self
            .runtime
            .block_on(self.client.execute_tag_bump(request))?)
    }

    pub(crate) fn list_tags(
        &self,
        request: v1::ListTagsRequest,
    ) -> anyhow::Result<v1::ListTagsResponse> {
        Ok(self.runtime.block_on(self.client.list_tags(request))?)
    }

    pub(crate) fn add_tag(&self, request: v1::AddTagRequest) -> anyhow::Result<v1::AddTagResponse> {
        Ok(self.runtime.block_on(self.client.add_tag(request))?)
    }

    pub(crate) fn push_tags(
        &self,
        request: v1::PushTagsRequest,
    ) -> anyhow::Result<v1::PushTagsResponse> {
        Ok(self.runtime.block_on(self.client.push_tags(request))?)
    }

    pub(crate) fn add_and_push_tag(
        &self,
        request: v1::AddAndPushTagRequest,
    ) -> anyhow::Result<v1::AddAndPushTagResponse> {
        Ok(self
            .runtime
            .block_on(self.client.add_and_push_tag(request))?)
    }

    pub(crate) fn label_tag(
        &self,
        request: v1::LabelTagRequest,
    ) -> anyhow::Result<v1::LabelTagResponse> {
        Ok(self.runtime.block_on(self.client.label_tag(request))?)
    }
}
