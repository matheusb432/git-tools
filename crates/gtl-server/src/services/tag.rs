use gtl_application::tags::{
    BumpLevel, TagActionStatus,
    bump_tag::{self, BumpTag, BumpTagOk},
    dry_run_tag_bump::{self, DryRunTagBump, DryRunTagBumpOk, TagBumpPreview},
};
use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitHead, TagName},
};
use gtl_wire::v1::{self, tag_service_server::TagService};
use tonic::{Request, Response, Status};

use super::{repository_root, required, run_blocking, unexpected};
use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct TagApi {
    state: AppState,
}

impl TagApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TagService for TagApi {
    async fn plan_bump(
        &self,
        request: Request<v1::PlanTagBumpRequest>,
    ) -> Result<Response<v1::PlanTagBumpResponse>, Status> {
        let request = plan_request(request.into_inner())?;
        let state = self.state.clone();
        let result = run_blocking(move || dry_run_tag_bump::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "plan tag bump"))?;

        Ok(Response::new(plan_response(result)))
    }

    async fn execute_bump(
        &self,
        request: Request<v1::ExecuteTagBumpRequest>,
    ) -> Result<Response<v1::ExecuteTagBumpResponse>, Status> {
        let preview = required(request.into_inner().preview, "preview")?;
        let request = BumpTag {
            preview: tag_bump_preview(preview)?,
        };
        let state = self.state.clone();
        let result = run_blocking(move || bump_tag::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "execute tag bump"))?;

        Ok(Response::new(execute_response(result)))
    }

    async fn list(
        &self,
        request: Request<v1::ListTagsRequest>,
    ) -> Result<Response<v1::ListTagsResponse>, Status> {
        basic::list(self.state.clone(), request.into_inner()).await
    }

    async fn add(
        &self,
        request: Request<v1::AddTagRequest>,
    ) -> Result<Response<v1::TagActionResponse>, Status> {
        basic::add(self.state.clone(), request.into_inner()).await
    }

    async fn push(
        &self,
        request: Request<v1::PushTagsRequest>,
    ) -> Result<Response<v1::TagActionResponse>, Status> {
        basic::push(self.state.clone(), request.into_inner()).await
    }

    async fn add_and_push(
        &self,
        request: Request<v1::AddAndPushTagRequest>,
    ) -> Result<Response<v1::TagActionResponse>, Status> {
        basic::add_and_push(self.state.clone(), request.into_inner()).await
    }

    async fn label(
        &self,
        request: Request<v1::LabelTagRequest>,
    ) -> Result<Response<v1::TagActionResponse>, Status> {
        basic::label(self.state.clone(), request.into_inner()).await
    }
}

fn plan_request(request: v1::PlanTagBumpRequest) -> Result<DryRunTagBump, Status> {
    Ok(DryRunTagBump {
        repo_path: super::absolute_path(request.repository_path, "repository_path")?,
        level: bump_level(request.level)?,
        message: request.message,
        push: request.push,
    })
}

fn bump_level(raw: i32) -> Result<BumpLevel, Status> {
    match v1::TagBumpLevel::try_from(raw)
        .map_err(|_| Status::invalid_argument("level is not recognized"))?
    {
        v1::TagBumpLevel::Unspecified => Err(Status::invalid_argument("level is required")),
        v1::TagBumpLevel::Major => Ok(BumpLevel::Major),
        v1::TagBumpLevel::Minor => Ok(BumpLevel::Minor),
        v1::TagBumpLevel::Patch => Ok(BumpLevel::Patch),
    }
}

fn wire_bump_level(level: BumpLevel) -> v1::TagBumpLevel {
    match level {
        BumpLevel::Major => v1::TagBumpLevel::Major,
        BumpLevel::Minor => v1::TagBumpLevel::Minor,
        BumpLevel::Patch => v1::TagBumpLevel::Patch,
    }
}

fn tag_bump_preview(preview: v1::TagBumpPreview) -> Result<TagBumpPreview, Status> {
    Ok(TagBumpPreview {
        repo_path: repository_root(preview.repository_root, "preview.repository_root")?,
        branch: git_head(required(preview.head, "preview.head")?)?,
        target_id: CommitId::try_from(preview.target_commit_id)
            .map_err(|error| Status::invalid_argument(error.to_string()))?,
        level: bump_level(preview.level)?,
        base_tag: TagName::try_new(preview.base_tag)
            .map_err(|_| Status::invalid_argument("preview.base_tag must not be empty"))?,
        next_tag: TagName::try_new(preview.next_tag)
            .map_err(|_| Status::invalid_argument("preview.next_tag must not be empty"))?,
        message: preview.message,
        push: preview.push,
    })
}

fn git_head(head: v1::GitHead) -> Result<GitHead, Status> {
    match required(head.state, "preview.head.state")? {
        v1::git_head::State::Branch(branch) => BranchName::try_new(branch)
            .map(GitHead::Branch)
            .map_err(|_| Status::invalid_argument("preview.head.branch must not be empty")),
        v1::git_head::State::Detached(_) => Ok(GitHead::Detached),
    }
}

fn wire_preview(preview: TagBumpPreview) -> v1::TagBumpPreview {
    let state = match preview.branch {
        GitHead::Branch(branch) => v1::git_head::State::Branch(branch.to_string()),
        GitHead::Detached => v1::git_head::State::Detached(v1::Empty {}),
    };
    v1::TagBumpPreview {
        repository_root: preview.repo_path.to_string(),
        head: Some(v1::GitHead { state: Some(state) }),
        target_commit_id: preview.target_id.to_string(),
        level: wire_bump_level(preview.level) as i32,
        base_tag: preview.base_tag.to_string(),
        next_tag: preview.next_tag.to_string(),
        message: preview.message,
        push: preview.push,
    }
}

fn plan_response(result: DryRunTagBumpOk) -> v1::PlanTagBumpResponse {
    let outcome = match result {
        DryRunTagBumpOk::Ready(preview) => {
            v1::plan_tag_bump_response::Outcome::Ready(wire_preview(preview))
        }
        DryRunTagBumpOk::Rejected { detail } => {
            v1::plan_tag_bump_response::Outcome::Rejected(v1::TagBumpRejection { detail })
        }
    };
    v1::PlanTagBumpResponse {
        outcome: Some(outcome),
    }
}

fn execute_response(result: BumpTagOk) -> v1::ExecuteTagBumpResponse {
    match result {
        BumpTagOk::Applied { tag, outcome } => {
            let detail = outcome.detail().to_owned();
            let status = match outcome.status() {
                TagActionStatus::Created => v1::TagBumpStatus::Created,
                TagActionStatus::Noop => v1::TagBumpStatus::NoOp,
                TagActionStatus::Pushed => v1::TagBumpStatus::Pushed,
                TagActionStatus::Failed => v1::TagBumpStatus::Failed,
            };
            let notes = (!detail.is_empty())
                .then(|| v1::Note {
                    level: if outcome.is_failed() {
                        v1::NoteLevel::Error
                    } else {
                        v1::NoteLevel::Info
                    } as i32,
                    text: detail.clone(),
                })
                .into_iter()
                .collect();
            v1::ExecuteTagBumpResponse {
                notes,
                outcome: Some(v1::execute_tag_bump_response::Outcome::Applied(
                    v1::TagBumpResult {
                        tag: tag.to_string(),
                        status: status as i32,
                        detail,
                    },
                )),
            }
        }
        BumpTagOk::Rejected { detail } => v1::ExecuteTagBumpResponse {
            notes: Vec::new(),
            outcome: Some(v1::execute_tag_bump_response::Outcome::Rejected(
                v1::TagBumpRejection { detail },
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_unspecified_bump_level_at_the_transport_boundary() {
        let error = bump_level(v1::TagBumpLevel::Unspecified as i32)
            .expect_err("unspecified level must fail");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(error.message(), "level is required");
    }

    #[test]
    fn preview_round_trip_preserves_detached_head() {
        let preview = TagBumpPreview {
            repo_path: repository_root("/repo".into(), "fixture").unwrap(),
            branch: GitHead::Detached,
            target_id: CommitId::try_from("a".repeat(40)).unwrap(),
            level: BumpLevel::Patch,
            base_tag: TagName::try_new("v1.0.0").unwrap(),
            next_tag: TagName::try_new("v1.0.1").unwrap(),
            message: "release".into(),
            push: true,
        };

        assert_eq!(
            tag_bump_preview(wire_preview(preview.clone())).unwrap(),
            preview
        );
    }
}
mod basic;
