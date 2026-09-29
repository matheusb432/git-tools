use std::path::PathBuf;

use super::{ViewerCodecError, decode_viewer_view_identity, encode_viewer_view_identity, required};
use crate::{
    proto::failure::{decode_failure, encode_failure},
    v1,
    viewer::push::{
        CreateViewerPush, ViewerPushCommandArgument, ViewerPushId, ViewerPushPreview,
        ViewerPushStatus,
    },
};

#[must_use]
pub fn encode_create(value: CreateViewerPush) -> v1::CreateViewerPushRequest {
    use v1::create_viewer_push_request::Source;
    v1::CreateViewerPushRequest {
        source: Some(match value {
            CreateViewerPush::Project { path } => Source::ProjectPath(path.to_string()),
            CreateViewerPush::View { identity } => {
                Source::Identity(encode_viewer_view_identity(identity))
            }
        }),
    }
}

pub fn decode_create(
    value: v1::CreateViewerPushRequest,
) -> Result<CreateViewerPush, ViewerCodecError> {
    use v1::create_viewer_push_request::Source;
    Ok(match required(value.source)? {
        Source::ProjectPath(path) => CreateViewerPush::Project {
            path: PathBuf::from(path)
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        },
        Source::Identity(identity) => CreateViewerPush::View {
            identity: decode_viewer_view_identity(identity)?,
        },
    })
}

pub fn decode_id(value: &str) -> Result<ViewerPushId, ViewerCodecError> {
    value.parse().map_err(|_| ViewerCodecError::InvalidMessage)
}

#[must_use]
pub fn encode_status(value: ViewerPushStatus) -> v1::GetViewerPushResponse {
    use v1::get_viewer_push_response::Status;
    v1::GetViewerPushResponse {
        status: Some(match value {
            ViewerPushStatus::Review(value) => Status::Review(v1::ViewerPushPreview {
                repository: value.repository.to_string(),
                project: value.project.map(|project| project.to_string()),
                branch: value.branch.to_string(),
                remote_branch: value.remote_branch.to_string(),
                remote: value.remote.to_string(),
                remote_url: value.remote_url.to_string(),
                commit: value.commit.to_string(),
                count: value.count,
                no_confirmation: value.no_confirmation,
                command: value.command,
                command_arguments: value
                    .command_arguments
                    .into_iter()
                    .map(|argument| encode_command_argument(argument) as i32)
                    .collect(),
            }),
            ViewerPushStatus::Running => Status::Running(v1::ViewerPushRunning {}),
            ViewerPushStatus::Queued => Status::Queued(v1::ViewerPushQueued {}),
            ViewerPushStatus::Succeeded => Status::Succeeded(v1::ViewerPushSucceeded {}),
            ViewerPushStatus::Failed { failure } => Status::Failed(encode_failure(&failure)),
        }),
    }
}

pub fn decode_status(
    value: v1::GetViewerPushResponse,
) -> Result<ViewerPushStatus, ViewerCodecError> {
    use v1::get_viewer_push_response::Status;
    Ok(match required(value.status)? {
        Status::Review(value) => ViewerPushStatus::Review(ViewerPushPreview {
            repository: PathBuf::from(value.repository)
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            project: value
                .project
                .map(TryInto::try_into)
                .transpose()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            branch: value
                .branch
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            remote_branch: value
                .remote_branch
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            remote: value
                .remote
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            remote_url: value
                .remote_url
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            commit: value
                .commit
                .parse()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            count: value.count,
            no_confirmation: value.no_confirmation,
            command: value.command,
            command_arguments: value
                .command_arguments
                .into_iter()
                .map(decode_command_argument)
                .collect::<Result<_, _>>()?,
        }),
        Status::Running(_) => ViewerPushStatus::Running,
        Status::Queued(_) => ViewerPushStatus::Queued,
        Status::Succeeded(_) => ViewerPushStatus::Succeeded,
        Status::Failed(failure) => ViewerPushStatus::Failed {
            failure: decode_failure(failure).ok_or(ViewerCodecError::InvalidMessage)?,
        },
    })
}

fn encode_command_argument(argument: ViewerPushCommandArgument) -> v1::ViewerPushCommandArgument {
    match argument {
        ViewerPushCommandArgument::Git => v1::ViewerPushCommandArgument::Git,
        ViewerPushCommandArgument::WorkingDirectory => {
            v1::ViewerPushCommandArgument::WorkingDirectory
        }
        ViewerPushCommandArgument::DisableMirroring => {
            v1::ViewerPushCommandArgument::DisableMirroring
        }
        ViewerPushCommandArgument::Push => v1::ViewerPushCommandArgument::Push,
        ViewerPushCommandArgument::Atomic => v1::ViewerPushCommandArgument::Atomic,
        ViewerPushCommandArgument::Porcelain => v1::ViewerPushCommandArgument::Porcelain,
        ViewerPushCommandArgument::NoFollowTags => v1::ViewerPushCommandArgument::NoFollowTags,
        ViewerPushCommandArgument::NoRecurseSubmodules => {
            v1::ViewerPushCommandArgument::NoRecurseSubmodules
        }
        ViewerPushCommandArgument::OptionSeparator => {
            v1::ViewerPushCommandArgument::OptionSeparator
        }
        ViewerPushCommandArgument::Remote => v1::ViewerPushCommandArgument::Remote,
        ViewerPushCommandArgument::CommitRef => v1::ViewerPushCommandArgument::CommitRef,
    }
}

fn decode_command_argument(value: i32) -> Result<ViewerPushCommandArgument, ViewerCodecError> {
    Ok(
        match v1::ViewerPushCommandArgument::try_from(value)
            .map_err(|_| ViewerCodecError::InvalidMessage)?
        {
            v1::ViewerPushCommandArgument::Unspecified => {
                return Err(ViewerCodecError::InvalidMessage);
            }
            v1::ViewerPushCommandArgument::Git => ViewerPushCommandArgument::Git,
            v1::ViewerPushCommandArgument::WorkingDirectory => {
                ViewerPushCommandArgument::WorkingDirectory
            }
            v1::ViewerPushCommandArgument::DisableMirroring => {
                ViewerPushCommandArgument::DisableMirroring
            }
            v1::ViewerPushCommandArgument::Push => ViewerPushCommandArgument::Push,
            v1::ViewerPushCommandArgument::Atomic => ViewerPushCommandArgument::Atomic,
            v1::ViewerPushCommandArgument::Porcelain => ViewerPushCommandArgument::Porcelain,
            v1::ViewerPushCommandArgument::NoFollowTags => ViewerPushCommandArgument::NoFollowTags,
            v1::ViewerPushCommandArgument::NoRecurseSubmodules => {
                ViewerPushCommandArgument::NoRecurseSubmodules
            }
            v1::ViewerPushCommandArgument::OptionSeparator => {
                ViewerPushCommandArgument::OptionSeparator
            }
            v1::ViewerPushCommandArgument::Remote => ViewerPushCommandArgument::Remote,
            v1::ViewerPushCommandArgument::CommitRef => ViewerPushCommandArgument::CommitRef,
        },
    )
}

#[must_use]
pub fn encode_availability_request(
    identity: crate::viewer::ViewerViewIdentity,
) -> v1::GetViewerPushAvailabilityRequest {
    v1::GetViewerPushAvailabilityRequest {
        identity: Some(encode_viewer_view_identity(identity)),
    }
}

pub fn decode_availability_request(
    value: v1::GetViewerPushAvailabilityRequest,
) -> Result<crate::viewer::ViewerViewIdentity, ViewerCodecError> {
    decode_viewer_view_identity(required(value.identity)?)
}

#[must_use]
pub fn encode_availability(
    value: crate::viewer::push::ViewerPushState,
) -> v1::GetViewerPushAvailabilityResponse {
    use v1::get_viewer_push_availability_response::Availability;

    use crate::viewer::push::ViewerPushAvailability;
    v1::GetViewerPushAvailabilityResponse {
        snapshot_has_unpushed_commits: value.snapshot_has_unpushed_commits,
        availability: Some(match value.availability {
            ViewerPushAvailability::Available => {
                Availability::Available(v1::ViewerPushAvailable {})
            }
            ViewerPushAvailability::NothingToPush => {
                Availability::NothingToPush(v1::ViewerPushNothingToPush {})
            }
            ViewerPushAvailability::Blocked { failure } => {
                Availability::Blocked(encode_failure(&failure))
            }
        }),
    }
}

pub fn decode_availability(
    value: v1::GetViewerPushAvailabilityResponse,
) -> Result<crate::viewer::push::ViewerPushState, ViewerCodecError> {
    use v1::get_viewer_push_availability_response::Availability;

    use crate::viewer::push::ViewerPushAvailability;
    Ok(crate::viewer::push::ViewerPushState {
        snapshot_has_unpushed_commits: value.snapshot_has_unpushed_commits,
        availability: match required(value.availability)? {
            Availability::Available(_) => ViewerPushAvailability::Available,
            Availability::NothingToPush(_) => ViewerPushAvailability::NothingToPush,
            Availability::Blocked(failure) => ViewerPushAvailability::Blocked {
                failure: decode_failure(failure).ok_or(ViewerCodecError::InvalidMessage)?,
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn review_round_trip_keeps_named_target_and_atomic_argument() {
        let review = ViewerPushStatus::Review(ViewerPushPreview {
            repository: PathBuf::from("/repos/example").try_into().unwrap(),
            project: Some("Example".to_owned().try_into().unwrap()),
            branch: "feature".to_owned().try_into().unwrap(),
            remote_branch: "main".to_owned().try_into().unwrap(),
            remote: "origin".to_owned().try_into().unwrap(),
            remote_url: "https://github.com/example/repo.git"
                .to_owned()
                .try_into()
                .unwrap(),
            commit: "a".repeat(40).parse().unwrap(),
            count: 2,
            no_confirmation: false,
            command: "git push --atomic".into(),
            command_arguments: vec![
                ViewerPushCommandArgument::Git,
                ViewerPushCommandArgument::Push,
                ViewerPushCommandArgument::Atomic,
            ],
        });
        assert_eq!(
            decode_status(encode_status(review.clone())).unwrap(),
            review
        );
    }
}
