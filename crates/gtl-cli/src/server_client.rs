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

    pub(crate) fn prepare_diff(
        &self,
        request: v1::PrepareDiffRequest,
    ) -> anyhow::Result<v1::PrepareDiffResponse> {
        Ok(self.runtime.block_on(self.client.prepare_diff(request))?)
    }

    pub(crate) fn prepare_merge_diff(
        &self,
        request: v1::PrepareMergeDiffRequest,
    ) -> anyhow::Result<v1::PrepareDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.prepare_merge_diff(request))?)
    }

    pub(crate) fn prepare_subrepositories_diff(
        &self,
        request: v1::PrepareSubrepositoriesDiffRequest,
    ) -> anyhow::Result<v1::PrepareDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.prepare_subrepositories_diff(request))?)
    }

    pub(crate) fn prepare_projects_diff(
        &self,
        request: v1::PrepareProjectsDiffRequest,
    ) -> anyhow::Result<v1::PrepareDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.prepare_projects_diff(request))?)
    }

    pub(crate) fn render_merge_diff(
        &self,
        request: v1::RenderMergeDiffRequest,
    ) -> anyhow::Result<v1::RenderDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.render_merge_diff(request))?)
    }

    pub(crate) fn render_subrepositories_diff(
        &self,
        request: v1::RenderSubrepositoriesDiffRequest,
    ) -> anyhow::Result<v1::RenderDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.render_subrepositories_diff(request))?)
    }

    pub(crate) fn render_projects_diff(
        &self,
        request: v1::RenderProjectsDiffRequest,
    ) -> anyhow::Result<v1::RenderDiffResponse> {
        Ok(self
            .runtime
            .block_on(self.client.render_projects_diff(request))?)
    }

    pub(crate) fn push_project_repositories(
        &self,
        request: v1::SyncProjectsRequest,
    ) -> anyhow::Result<v1::SyncProjectsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.push_project_repositories(request))?)
    }

    pub(crate) fn pull_project_repositories(
        &self,
        request: v1::SyncProjectsRequest,
    ) -> anyhow::Result<v1::SyncProjectsResponse> {
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
    ) -> anyhow::Result<v1::RepositoryStatusesResponse> {
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

    pub(crate) fn plan_recursive_push(
        &self,
        request: v1::PlanRecursivePushRequest,
    ) -> anyhow::Result<v1::PlanRecursivePushResponse> {
        Ok(self
            .runtime
            .block_on(self.client.plan_recursive_push(request))?)
    }

    pub(crate) fn execute_recursive_push(
        &self,
        request: v1::ExecuteRecursivePushRequest,
    ) -> anyhow::Result<v1::ExecuteRecursivePushResponse> {
        Ok(self
            .runtime
            .block_on(self.client.execute_recursive_push(request))?)
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
    ) -> anyhow::Result<v1::RepositoryStatusesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_repository_status(request))?)
    }

    pub(crate) fn get_recursive_repository_statuses(
        &self,
        request: v1::GetRecursiveRepositoryStatusesRequest,
    ) -> anyhow::Result<v1::RepositoryStatusesResponse> {
        Ok(self
            .runtime
            .block_on(self.client.get_recursive_repository_statuses(request))?)
    }

    pub(crate) fn get_settings(&self) -> anyhow::Result<v1::GetSettingsResponse> {
        Ok(self.runtime.block_on(self.client.get_settings())?)
    }

    pub(crate) fn set_theme(
        &self,
        request: v1::SetThemeRequest,
    ) -> anyhow::Result<v1::SetThemeResponse> {
        Ok(self.runtime.block_on(self.client.set_theme(request))?)
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

    pub(crate) fn save_live_view(
        &self,
        request: v1::SaveLiveViewRequest,
    ) -> anyhow::Result<v1::SaveLiveViewResponse> {
        Ok(self.runtime.block_on(self.client.save_live_view(request))?)
    }

    pub(crate) fn save_project_live_views(
        &self,
    ) -> anyhow::Result<v1::SaveProjectLiveViewsResponse> {
        Ok(self
            .runtime
            .block_on(self.client.save_project_live_views())?)
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

    pub(crate) fn add_tag(
        &self,
        request: v1::AddTagRequest,
    ) -> anyhow::Result<v1::TagActionResponse> {
        Ok(self.runtime.block_on(self.client.add_tag(request))?)
    }

    pub(crate) fn push_tags(
        &self,
        request: v1::PushTagsRequest,
    ) -> anyhow::Result<v1::TagActionResponse> {
        Ok(self.runtime.block_on(self.client.push_tags(request))?)
    }

    pub(crate) fn add_and_push_tag(
        &self,
        request: v1::AddAndPushTagRequest,
    ) -> anyhow::Result<v1::TagActionResponse> {
        Ok(self
            .runtime
            .block_on(self.client.add_and_push_tag(request))?)
    }

    pub(crate) fn label_tag(
        &self,
        request: v1::LabelTagRequest,
    ) -> anyhow::Result<v1::TagActionResponse> {
        Ok(self.runtime.block_on(self.client.label_tag(request))?)
    }
}
