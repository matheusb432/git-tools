use gtl_client::GtlClient;
use gtl_wire::v1;

pub(crate) struct ServerClient {
    runtime: tokio::runtime::Runtime,
    client: GtlClient,
}

pub(crate) struct ServerStatus {
    pub(crate) address: std::net::SocketAddr,
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

    pub(crate) fn status(&self) -> ServerStatus {
        ServerStatus {
            address: self.client.endpoint().address(),
            instance_id: self.client.endpoint().instance_id().to_string(),
        }
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

    pub(crate) fn commit_project_repositories(
        &self,
        request: v1::CommitProjectRepositoriesRequest,
    ) -> anyhow::Result<v1::CommitProjectRepositoriesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.commit_project_repositories(request))?)
    }

    pub(crate) fn prune_project_branches(
        &self,
        request: v1::PruneProjectBranchesRequest,
    ) -> anyhow::Result<v1::PruneProjectBranchesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.prune_project_branches(request))?)
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

    pub(crate) fn plan_repository_commit(
        &self,
        request: v1::PlanRepositoryCommitRequest,
    ) -> anyhow::Result<v1::PlanRepositoryCommitResponse> {
        Ok(self
            .runtime
            .block_on(self.client.plan_repository_commit(request))?)
    }

    pub(crate) fn execute_repository_commit(
        &self,
        request: v1::ExecuteRepositoryCommitRequest,
    ) -> anyhow::Result<v1::ExecuteRepositoryCommitResponse> {
        Ok(self
            .runtime
            .block_on(self.client.execute_repository_commit(request))?)
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

    pub(crate) fn change_repository_branch(
        &self,
        request: v1::ChangeRepositoryBranchRequest,
    ) -> anyhow::Result<v1::ChangeRepositoryBranchResponse> {
        Ok(self
            .runtime
            .block_on(self.client.change_repository_branch(request))?)
    }

    pub(crate) fn plan_repository_prune(
        &self,
        request: v1::PlanRepositoryPruneRequest,
    ) -> anyhow::Result<v1::PlanRepositoryPruneResponse> {
        Ok(self
            .runtime
            .block_on(self.client.plan_repository_prune(request))?)
    }

    pub(crate) fn execute_repository_prune(
        &self,
        request: v1::ExecuteRepositoryPruneRequest,
    ) -> anyhow::Result<v1::ExecuteRepositoryPruneResponse> {
        Ok(self
            .runtime
            .block_on(self.client.execute_repository_prune(request))?)
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

    pub(crate) fn get_worktree_base(
        &self,
        request: v1::GetWorktreeBaseRequest,
    ) -> anyhow::Result<v1::GetWorktreeBaseResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_worktree_base(request))?)
    }

    pub(crate) fn list_worktrees(
        &self,
        request: v1::ListWorktreesRequest,
    ) -> anyhow::Result<v1::ListWorktreesResponse> {
        Ok(self.runtime.block_on(self.client.list_worktrees(request))?)
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
