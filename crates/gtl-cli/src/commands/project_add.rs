use std::str::FromStr;

use anyhow::{Context as _, Result};
use gtl_models::{
    git::RemoteUrl,
    projects::catalogue::{
        ProjectColor, ProjectDirectorySource, ProjectGroupName, ProjectGroups, ProjectId,
        ProjectTitle,
    },
};
use gtl_wire::v1;
use serde::Deserialize;

use crate::server_client::ServerClient;

/// Parses project creation JSON into a validated RPC request before connecting.
#[derive(Clone, Debug)]
pub struct ProjectAddPayload(v1::CreateProjectRequest);

impl FromStr for ProjectAddPayload {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !value.trim_start().starts_with('{') {
            return Err("project add JSON must be one object".to_owned());
        }
        let payload: AddPayload = serde_json::from_str(value)
            .map_err(|error| format!("project add JSON is invalid: {error}"))?;
        let project_id = ProjectId::try_new(payload.project_id.to_ascii_uppercase())
            .map_err(|_| "project_id must contain 2 to 4 ASCII letters".to_owned())?;
        let title = ProjectTitle::try_new(payload.title)
            .map_err(|error| format!("invalid title: {error}"))?;
        let SourcePayload::Directory { path } = payload.source;
        let source = ProjectDirectorySource::try_new(path)
            .map_err(|error| format!("invalid source.path: {error}"))?;
        let git_remote = payload
            .git_remote
            .map(RemoteUrl::try_new)
            .transpose()
            .map_err(|error| format!("invalid git_remote: {error}"))?;
        let color = payload
            .color
            .map(ProjectColor::try_new)
            .transpose()
            .map_err(|error| format!("invalid color: {error}"))?;
        let groups = payload
            .groups
            .into_iter()
            .map(ProjectGroupName::try_new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("invalid groups: {error}"))?;
        let groups =
            ProjectGroups::try_new(groups).map_err(|error| format!("invalid groups: {error}"))?;

        Ok(Self(v1::CreateProjectRequest {
            project_id: project_id.to_string(),
            project: Some(v1::ProjectCreation {
                title: title.to_string(),
                source: Some(v1::ProjectSource {
                    source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                        path: source.to_string(),
                    })),
                }),
                git_remote: git_remote.map(|remote| remote.to_string()),
                color: color.map(|color| color.to_string()),
                groups: groups.as_slice().iter().map(ToString::to_string).collect(),
                include_in_full_export: payload.include_in_full_export,
            }),
        }))
    }
}

pub(crate) fn run(payload: ProjectAddPayload, json: bool) -> Result<String> {
    let response = ServerClient::connect()?.create_project(payload.0)?;
    let project_id = ProjectId::try_new(response.project_id)
        .context("gtl-server returned an invalid created project ID")?;
    if json {
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "project_id": project_id.as_ref(),
            "status": "active",
        }))?);
    }
    Ok(format!("Added project: {project_id}"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddPayload {
    project_id: String,
    title: String,
    source: SourcePayload,
    git_remote: Option<String>,
    color: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
    include_in_full_export: Option<bool>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SourcePayload {
    Directory { path: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_inclusion_preserves_omission_and_explicit_choices() {
        let payload = serde_json::json!({
            "project_id": "APP",
            "title": "My app",
            "source": {"kind": "directory", "path": std::env::temp_dir()},
        });
        for choice in [None, Some(false), Some(true)] {
            let mut payload = payload.clone();
            if let Some(choice) = choice {
                payload["include_in_full_export"] = serde_json::json!(choice);
            }
            let parsed = payload.to_string().parse::<ProjectAddPayload>().unwrap();
            assert_eq!(parsed.0.project.unwrap().include_in_full_export, choice);
        }
    }
}
