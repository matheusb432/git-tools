use gtl_application::{
    repositories::resolve_repository_root,
    tags::{
        BumpLevel, ListTagsOk, TagActionOutcome, TagActionStatus, TagGroup, TagOperationProgress,
        TagRemotePushProgress,
        add_and_push_tag::{self, AddAndPushTag, AddAndPushTagError},
        add_tag::{self, AddTag, AddTagError},
        bump_tag::{self, BumpTagOk},
        dry_run_tag_bump::{self, DryRunTagBump, DryRunTagBumpOk, TagBumpPreview},
        label_tag::{self, LabelTag, LabelTagError},
        list_tags::{self, ListTags},
        push_tags::{self, PushTagsError},
    },
};
use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitHead, TagName},
    paths::RepositoryRoot,
    tags::{Tag, TagState},
};
use gtl_wire::v1::{self, tag_service_server::TagService};
use tonic::{Request, Response, Status};

use super::{absolute_path, repository_root, required, run_blocking, unexpected};
use crate::state::AppState;

pub(crate) struct TagGrpcService {
    state: AppState,
}

impl TagGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TagService for TagGrpcService {
    async fn plan_tag_bump(
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

    async fn execute_tag_bump(
        &self,
        request: Request<v1::ExecuteTagBumpRequest>,
    ) -> Result<Response<v1::ExecuteTagBumpResponse>, Status> {
        let preview = required(request.into_inner().preview, "preview")?;
        let request = tag_bump_preview(preview)?;
        let state = self.state.clone();
        let result = run_blocking(move || bump_tag::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "execute tag bump"))?;

        Ok(Response::new(execute_response(result)))
    }

    async fn list_tags(
        &self,
        request: Request<v1::ListTagsRequest>,
    ) -> Result<Response<v1::ListTagsResponse>, Status> {
        let request = request.into_inner();
        let repo_path = absolute_path(request.repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repo_path = resolve_root(repo_path, &state)?;
            list_tags::execute(
                ListTags {
                    repo_path,
                    include_state: request.include_state,
                },
                &state.git,
            )
            .map_err(anyhow::Error::from)
        })
        .await?
        .map_err(|error| unexpected(error, "list repository tags"))?;
        let outcome = match result {
            ListTagsOk::Listed { groups } => {
                v1::list_tags_response::Outcome::Listed(v1::TagGroups {
                    groups: groups.into_iter().map(wire_group).collect(),
                })
            }
            ListTagsOk::Failed { detail } => {
                v1::list_tags_response::Outcome::Failed(v1::TagListFailure { detail })
            }
        };

        Ok(Response::new(v1::ListTagsResponse {
            outcome: Some(outcome),
        }))
    }

    async fn add_tag(
        &self,
        request: Request<v1::AddTagRequest>,
    ) -> Result<Response<v1::AddTagResponse>, Status> {
        let request = request.into_inner();
        let repo_path = absolute_path(request.repository_path, "repository_path")?;
        let tag = tag_name(request.tag, "tag")?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repo_path = resolve_root(repo_path, &state)?;
            Ok::<_, anyhow::Error>(
                match add_tag::execute(
                    AddTag {
                        repo_path,
                        tag,
                        message: request.message,
                    },
                    &state.git,
                ) {
                    Ok(outcome) => action_response(&outcome),
                    Err(error) => add_aborted(error),
                },
            )
        })
        .await?
        .map_err(|error| unexpected(error, "add repository tag"))?;

        Ok(Response::new(result.into()))
    }

    async fn push_tags(
        &self,
        request: Request<v1::PushTagsRequest>,
    ) -> Result<Response<v1::PushTagsResponse>, Status> {
        let request = request.into_inner();
        let repo_path = absolute_path(request.repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repo_path = resolve_root(repo_path, &state)?;
            Ok::<_, anyhow::Error>(match push_tags::execute(&repo_path, &state.git) {
                Ok(outcome) => action_response(&outcome),
                Err(error) => push_aborted(error),
            })
        })
        .await?
        .map_err(|error| unexpected(error, "push repository tags"))?;

        Ok(Response::new(result.into()))
    }

    async fn add_and_push_tag(
        &self,
        request: Request<v1::AddAndPushTagRequest>,
    ) -> Result<Response<v1::AddAndPushTagResponse>, Status> {
        let request = request.into_inner();
        let repo_path = absolute_path(request.repository_path, "repository_path")?;
        let tag = tag_name(request.tag, "tag")?;
        let label = request
            .label
            .map(|label| tag_name(label, "label"))
            .transpose()?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repo_path = resolve_root(repo_path, &state)?;
            Ok::<_, anyhow::Error>(
                match add_and_push_tag::execute(
                    AddAndPushTag {
                        repo_path,
                        tag,
                        message: request.message,
                        label,
                    },
                    &state.git,
                ) {
                    Ok(outcome) => action_response(&outcome),
                    Err(error) => add_and_push_aborted(error),
                },
            )
        })
        .await?
        .map_err(|error| unexpected(error, "add and push repository tag"))?;

        Ok(Response::new(result.into()))
    }

    async fn label_tag(
        &self,
        request: Request<v1::LabelTagRequest>,
    ) -> Result<Response<v1::LabelTagResponse>, Status> {
        let request = request.into_inner();
        let repo_path = absolute_path(request.repository_path, "repository_path")?;
        let tag = tag_name(request.tag, "tag")?;
        let label = tag_name(request.label, "label")?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repo_path = resolve_root(repo_path, &state)?;
            Ok::<_, anyhow::Error>(
                match label_tag::execute(
                    LabelTag {
                        repo_path,
                        tag,
                        label,
                    },
                    &state.git,
                ) {
                    Ok(outcome) => action_response(&outcome),
                    Err(error) => label_aborted(error),
                },
            )
        })
        .await?
        .map_err(|error| unexpected(error, "label repository tag"))?;

        Ok(Response::new(result.into()))
    }
}

fn plan_request(request: v1::PlanTagBumpRequest) -> Result<DryRunTagBump, Status> {
    Ok(DryRunTagBump {
        repo_path: absolute_path(request.repository_path, "repository_path")?,
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

fn resolve_root(repo_path: std::path::PathBuf, state: &AppState) -> anyhow::Result<RepositoryRoot> {
    resolve_repository_root::execute(repo_path, &state.git).map_err(anyhow::Error::from)
}

fn tag_name(raw: String, field: &'static str) -> Result<TagName, Status> {
    TagName::try_new(raw)
        .map_err(|_| Status::invalid_argument(format!("{field} must not be empty")))
}

fn wire_group(group: TagGroup) -> v1::TagGroup {
    let kind = match group {
        TagGroup::Single(tag) => v1::tag_group::Kind::Single(wire_tag(&tag)),
        TagGroup::Canonical { canonical, labels } => {
            v1::tag_group::Kind::Canonical(v1::CanonicalTagGroup {
                canonical: Some(wire_tag(&canonical)),
                labels: labels.iter().map(wire_tag).collect(),
            })
        }
        TagGroup::MoreThanOneTagHasMessage(tags) => {
            v1::tag_group::Kind::MultipleAnnotated(v1::TagCollection {
                tags: tags.iter().map(wire_tag).collect(),
            })
        }
        TagGroup::AllLabels(tags) => v1::tag_group::Kind::AllLabels(v1::TagCollection {
            tags: tags.iter().map(wire_tag).collect(),
        }),
    };
    v1::TagGroup { kind: Some(kind) }
}

fn wire_tag(tag: &Tag) -> v1::Tag {
    v1::Tag {
        name: tag.name().to_string(),
        object_id: tag.object().to_string(),
        commit_id: tag.commit().to_string(),
        created_at: tag.created_at().map(ToString::to_string),
        annotated: tag.is_annotated(),
        message: tag.message().map(str::to_owned),
        state: match tag.state() {
            None => v1::TagState::NotQueried,
            Some(TagState::Local) => v1::TagState::Local,
            Some(TagState::Remote) => v1::TagState::Remote,
        } as i32,
    }
}

struct TagActionResult {
    status: i32,
    detail: String,
    progress: Option<v1::TagOperationProgress>,
}

fn action_response(outcome: &TagActionOutcome) -> TagActionResult {
    TagActionResult {
        status: match outcome.status() {
            TagActionStatus::Created => v1::TagActionStatus::Created,
            TagActionStatus::Noop => v1::TagActionStatus::NoOp,
            TagActionStatus::Pushed => v1::TagActionStatus::Pushed,
            TagActionStatus::Failed => v1::TagActionStatus::Failed,
        } as i32,
        detail: outcome.detail().to_owned(),
        progress: Some(wire_progress(outcome.progress())),
    }
}

fn aborted(detail: String, progress: &TagOperationProgress) -> TagActionResult {
    TagActionResult {
        status: v1::TagActionStatus::Aborted as i32,
        detail,
        progress: Some(wire_progress(progress)),
    }
}

fn add_aborted(error: AddTagError) -> TagActionResult {
    let detail = error.to_string();
    match error {
        AddTagError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn push_aborted(error: PushTagsError) -> TagActionResult {
    let detail = error.to_string();
    match error {
        PushTagsError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn add_and_push_aborted(error: AddAndPushTagError) -> TagActionResult {
    let detail = error.to_string();
    match error {
        AddAndPushTagError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn label_aborted(error: LabelTagError) -> TagActionResult {
    let detail = error.to_string();
    match error {
        LabelTagError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn wire_progress(progress: &TagOperationProgress) -> v1::TagOperationProgress {
    let (remote_push, pushed_or_attempted_refs) = match &progress.remote_push {
        TagRemotePushProgress::NotStarted => (v1::TagRemotePushProgress::NotStarted, Vec::new()),
        TagRemotePushProgress::Completed { pushed_refs } => (
            v1::TagRemotePushProgress::Completed,
            pushed_refs.iter().map(ToString::to_string).collect(),
        ),
        TagRemotePushProgress::Indeterminate { attempted_refs } => (
            v1::TagRemotePushProgress::Indeterminate,
            attempted_refs.iter().map(ToString::to_string).collect(),
        ),
    };
    v1::TagOperationProgress {
        created_refs: progress
            .created_refs
            .iter()
            .map(ToString::to_string)
            .collect(),
        remote_push: remote_push as i32,
        pushed_or_attempted_refs,
    }
}

impl From<TagActionResult> for v1::AddTagResponse {
    fn from(response: TagActionResult) -> Self {
        Self {
            status: response.status,
            detail: response.detail,
            progress: response.progress,
        }
    }
}

impl From<TagActionResult> for v1::PushTagsResponse {
    fn from(response: TagActionResult) -> Self {
        Self {
            status: response.status,
            detail: response.detail,
            progress: response.progress,
        }
    }
}

impl From<TagActionResult> for v1::AddAndPushTagResponse {
    fn from(response: TagActionResult) -> Self {
        Self {
            status: response.status,
            detail: response.detail,
            progress: response.progress,
        }
    }
}

impl From<TagActionResult> for v1::LabelTagResponse {
    fn from(response: TagActionResult) -> Self {
        Self {
            status: response.status,
            detail: response.detail,
            progress: response.progress,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_unspecified_bump_level_at_the_transport_boundary() {
        let error = bump_level(v1::TagBumpLevel::Unspecified as i32).unwrap_err();

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
