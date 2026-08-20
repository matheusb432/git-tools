use gtl_application::{
    repositories::resolve_repository_root,
    tags::{
        ListTagsOk, TagActionOutcome, TagActionStatus, TagGroup, TagOperationProgress,
        TagRemotePushProgress,
        add_and_push_tag::{self, AddAndPushTag, AddAndPushTagError},
        add_tag::{self, AddTag, AddTagError},
        label_tag::{self, LabelTag, LabelTagError},
        list_tags::{self, ListTags},
        push_tags::{self, PushTags, PushTagsError},
    },
};
use gtl_models::{
    git::TagName,
    paths::RepositoryRoot,
    tags::{Tag, TagState},
};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, task_join, unexpected};
use crate::state::AppState;

pub(super) async fn list(
    state: AppState,
    request: v1::ListTagsRequest,
) -> Result<Response<v1::ListTagsResponse>, Status> {
    let repo_path = absolute_path(request.repository_path, "repository_path")?;
    let result = tokio::task::spawn_blocking(move || {
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
    .await
    .map_err(|error| task_join(&error))?
    .map_err(|error| unexpected(error, "list repository tags"))?;
    let outcome = match result {
        ListTagsOk::Listed { groups } => v1::list_tags_response::Outcome::Listed(v1::TagGroups {
            groups: groups.into_iter().map(wire_group).collect(),
        }),
        ListTagsOk::Failed { detail } => {
            v1::list_tags_response::Outcome::Failed(v1::TagListFailure { detail })
        }
    };
    Ok(Response::new(v1::ListTagsResponse {
        outcome: Some(outcome),
    }))
}

pub(super) async fn add(
    state: AppState,
    request: v1::AddTagRequest,
) -> Result<Response<v1::TagActionResponse>, Status> {
    let repo_path = absolute_path(request.repository_path, "repository_path")?;
    let tag = tag_name(request.tag, "tag")?;
    let result = tokio::task::spawn_blocking(move || {
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
    .await
    .map_err(|error| task_join(&error))?
    .map_err(|error| unexpected(error, "add repository tag"))?;
    Ok(Response::new(result))
}

pub(super) async fn push(
    state: AppState,
    request: v1::PushTagsRequest,
) -> Result<Response<v1::TagActionResponse>, Status> {
    let repo_path = absolute_path(request.repository_path, "repository_path")?;
    let result = tokio::task::spawn_blocking(move || {
        let repo_path = resolve_root(repo_path, &state)?;
        Ok::<_, anyhow::Error>(
            match push_tags::execute(PushTags { repo_path }, &state.git) {
                Ok(outcome) => action_response(&outcome),
                Err(error) => push_aborted(error),
            },
        )
    })
    .await
    .map_err(|error| task_join(&error))?
    .map_err(|error| unexpected(error, "push repository tags"))?;
    Ok(Response::new(result))
}

pub(super) async fn add_and_push(
    state: AppState,
    request: v1::AddAndPushTagRequest,
) -> Result<Response<v1::TagActionResponse>, Status> {
    let repo_path = absolute_path(request.repository_path, "repository_path")?;
    let tag = tag_name(request.tag, "tag")?;
    let label = request
        .label
        .map(|label| tag_name(label, "label"))
        .transpose()?;
    let result = tokio::task::spawn_blocking(move || {
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
    .await
    .map_err(|error| task_join(&error))?
    .map_err(|error| unexpected(error, "add and push repository tag"))?;
    Ok(Response::new(result))
}

pub(super) async fn label(
    state: AppState,
    request: v1::LabelTagRequest,
) -> Result<Response<v1::TagActionResponse>, Status> {
    let repo_path = absolute_path(request.repository_path, "repository_path")?;
    let tag = tag_name(request.tag, "tag")?;
    let label = tag_name(request.label, "label")?;
    let result = tokio::task::spawn_blocking(move || {
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
    .await
    .map_err(|error| task_join(&error))?
    .map_err(|error| unexpected(error, "label repository tag"))?;
    Ok(Response::new(result))
}

fn resolve_root(repo_path: std::path::PathBuf, state: &AppState) -> anyhow::Result<RepositoryRoot> {
    resolve_repository_root::execute(
        resolve_repository_root::ResolveRepositoryRoot { repo_path },
        &state.git,
    )
    .map_err(anyhow::Error::from)
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

fn action_response(outcome: &TagActionOutcome) -> v1::TagActionResponse {
    v1::TagActionResponse {
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

fn aborted(detail: String, progress: &TagOperationProgress) -> v1::TagActionResponse {
    v1::TagActionResponse {
        status: v1::TagActionStatus::Aborted as i32,
        detail,
        progress: Some(wire_progress(progress)),
    }
}

fn add_aborted(error: AddTagError) -> v1::TagActionResponse {
    let detail = error.to_string();
    match error {
        AddTagError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn push_aborted(error: PushTagsError) -> v1::TagActionResponse {
    let detail = error.to_string();
    match error {
        PushTagsError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn add_and_push_aborted(error: AddAndPushTagError) -> v1::TagActionResponse {
    let detail = error.to_string();
    match error {
        AddAndPushTagError::Unexpected { progress, .. } => aborted(detail, &progress),
        _ => aborted(detail, &TagOperationProgress::default()),
    }
}

fn label_aborted(error: LabelTagError) -> v1::TagActionResponse {
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
