use std::path::PathBuf;

use super::{ViewerCodecError, decode_viewer_view_identity, encode_viewer_view_identity, required};
use crate::{
    v1,
    viewer::push::{CreateViewerPush, ViewerPushId, ViewerPushPreview, ViewerPushStatus},
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
                destination: value.destination,
                commit: value.commit.to_string(),
                count: value.count,
                command: value.command,
            }),
            ViewerPushStatus::Running => Status::Running(v1::ViewerPushRunning {}),
            ViewerPushStatus::Queued => Status::Queued(v1::ViewerPushQueued {}),
            ViewerPushStatus::Succeeded => Status::Succeeded(v1::ViewerPushSucceeded {}),
            ViewerPushStatus::Failed { message } => Status::Failure(message),
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
            destination: value.destination,
            commit: value
                .commit
                .parse()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            count: value.count,
            command: value.command,
        }),
        Status::Running(_) => ViewerPushStatus::Running,
        Status::Queued(_) => ViewerPushStatus::Queued,
        Status::Succeeded(_) => ViewerPushStatus::Succeeded,
        Status::Failure(message) => ViewerPushStatus::Failed { message },
    })
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
    value: crate::viewer::push::ViewerPushAvailability,
) -> v1::GetViewerPushAvailabilityResponse {
    use v1::get_viewer_push_availability_response::Availability;

    use crate::viewer::push::ViewerPushAvailability;
    v1::GetViewerPushAvailabilityResponse {
        availability: Some(match value {
            ViewerPushAvailability::Available => {
                Availability::Available(v1::ViewerPushAvailable {})
            }
            ViewerPushAvailability::NothingToPush => {
                Availability::NothingToPush(v1::ViewerPushNothingToPush {})
            }
            ViewerPushAvailability::Unavailable { message } => Availability::Unavailable(message),
        }),
    }
}

pub fn decode_availability(
    value: v1::GetViewerPushAvailabilityResponse,
) -> Result<crate::viewer::push::ViewerPushAvailability, ViewerCodecError> {
    use v1::get_viewer_push_availability_response::Availability;

    use crate::viewer::push::ViewerPushAvailability;
    Ok(match required(value.availability)? {
        Availability::Available(_) => ViewerPushAvailability::Available,
        Availability::NothingToPush(_) => ViewerPushAvailability::NothingToPush,
        Availability::Unavailable(message) => ViewerPushAvailability::Unavailable { message },
    })
}
