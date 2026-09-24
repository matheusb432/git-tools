use gtl_application::viewer::get_viewer_shell;
use gtl_models::failure::{ErrorClass, ViewerFailure};
use gtl_wire::{
    proto, v1,
    viewer::{ViewerFeedback, ViewerShell, ViewerViewIdentity},
};
use tonic::Status;

use super::super::status::{GrpcResultExt as _, invalid_request, private, status};
use crate::{state::AppState, viewer_runtime};

pub(super) fn project_shell(
    state: &AppState,
    feedback: Option<ViewerFeedback>,
) -> Result<v1::ViewerShell, Status> {
    let result =
        get_viewer_shell::execute(feedback, &state.viewer, &state.user_settings).into_grpc()?;
    if let Some(work) = result.commit_reload {
        viewer_runtime::spawn_commit(state.clone(), work);
    }
    if let Some(work) = result.full_context {
        viewer_runtime::spawn_full_context(state.clone(), work);
    }
    project_shell_proto(result.shell)
}

pub(super) fn project_shell_proto(shell: ViewerShell) -> Result<v1::ViewerShell, Status> {
    proto::viewer::encode_viewer_shell(shell).map_err(|error| match error {
        proto::viewer::ViewerCodecError::Unrepresentable => {
            status(&ViewerFailure::ResponseTooLarge)
        }
        proto::viewer::ViewerCodecError::InvalidMessage
        | proto::viewer::ViewerCodecError::InvalidField { .. } => {
            private(ErrorClass::Internal, "viewer shell encoding failed")
        }
    })
}

pub(super) fn parse_identity(
    identity: &v1::ViewerViewIdentity,
) -> Result<ViewerViewIdentity, Status> {
    proto::viewer::decode_viewer_view_identity(*identity).map_err(|_| invalid_request("identity"))
}
