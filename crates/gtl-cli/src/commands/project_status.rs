use anyhow::{Result, bail};
use gtl_models::projects::catalogue::ProjectId;
use gtl_wire::v1;
use serde::Serialize;

use crate::server_client::ServerClient;

#[derive(Clone, Copy)]
pub enum ProjectStatusAction {
    Pause,
    Resume,
}

impl ProjectStatusAction {
    #[must_use]
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Pause => "project pause",
            Self::Resume => "project resume",
        }
    }

    const fn text(self, changed: bool) -> &'static str {
        match (self, changed) {
            (Self::Pause, true) => "Paused",
            (Self::Pause, false) => "Already paused",
            (Self::Resume, true) => "Resumed",
            (Self::Resume, false) => "Already active",
        }
    }
}

#[derive(Serialize)]
struct ProjectStatusChange<'a> {
    project_id: &'a str,
    status: &'static str,
    changed: bool,
}

pub fn run(action: ProjectStatusAction, id: &ProjectId, json: bool) -> Result<String> {
    let client = ServerClient::connect()?;
    let mode = v1::ProjectOperationMode::Apply.into();
    let (outcome, target_status) = match action {
        ProjectStatusAction::Pause => {
            let response = client.pause_project(v1::PauseProjectRequest {
                project_id: id.to_string(),
                mode,
            })?;
            (response.outcome(), response.target_status())
        }
        ProjectStatusAction::Resume => {
            let response = client.resume_project(v1::ResumeProjectRequest {
                project_id: id.to_string(),
                mode,
            })?;
            (response.outcome(), response.target_status())
        }
    };
    let changed = match outcome {
        v1::ProjectMutationOutcome::Changed => true,
        v1::ProjectMutationOutcome::Unchanged => false,
        v1::ProjectMutationOutcome::Unspecified => {
            bail!("gtl-server returned an invalid project mutation outcome")
        }
    };
    let status = match target_status {
        v1::ProjectStatus::Active => "active",
        v1::ProjectStatus::Paused => "paused",
        v1::ProjectStatus::Unspecified => {
            bail!("gtl-server returned an invalid project status")
        }
    };
    if json {
        return Ok(serde_json::to_string_pretty(&ProjectStatusChange {
            project_id: id.as_ref(),
            status,
            changed,
        })?);
    }
    Ok(format!("{} project: {id}", action.text(changed)))
}
