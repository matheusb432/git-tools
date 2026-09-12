use gtl_models::paths::RepositoryRoot;
use gtl_wire::v1;

use crate::{
    ExitCode,
    cli::TagBumpLevel,
    commands,
    confirm::{Detail, Dialog},
    output,
    server_client::ServerClient,
};

pub struct BumpArgs {
    pub message: String,
    pub level: TagBumpLevel,
    pub pattern: Option<String>,
    pub push: bool,
    pub dry: bool,
    pub yes: bool,
}

#[must_use]
pub fn run(args: BumpArgs) -> ExitCode {
    let BumpArgs {
        message,
        level,
        pattern,
        push,
        dry,
        yes,
    } = args;
    let (client, preview) = match prepare_bump(level, pattern, message, push) {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("tag bump: {}", crate::error_text(&error));
            return ExitCode::Internal;
        }
    };

    let dialog = match confirmation(&preview) {
        Ok(dialog) => dialog,
        Err(error) => {
            eprintln!("tag bump: {}", crate::error_text(&error));
            return ExitCode::Internal;
        }
    };
    if dry {
        println!(
            "{}",
            dialog.render("Tag bump preview", output::stdout_color())
        );
        return ExitCode::Ok;
    }
    if let Err(exit) = crate::confirm::request("tag bump", yes, &dialog) {
        return exit;
    }

    let bumped = match client.execute_tag_bump(v1::ExecuteTagBumpRequest {
        preview: Some(preview),
    }) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("tag bump: {}", crate::error_text(&error));
            return ExitCode::Internal;
        }
    };
    match bumped.outcome {
        Some(v1::execute_tag_bump_response::Outcome::Applied(result)) => {
            let status = match v1::TagBumpStatus::try_from(result.status) {
                Ok(v1::TagBumpStatus::Created) => v1::TagActionStatus::Created,
                Ok(v1::TagBumpStatus::NoOp) => v1::TagActionStatus::NoOp,
                Ok(v1::TagBumpStatus::Pushed) => v1::TagActionStatus::Pushed,
                Ok(v1::TagBumpStatus::Failed) => v1::TagActionStatus::Failed,
                Ok(v1::TagBumpStatus::Unspecified) | Err(_) => {
                    eprintln!("tag bump: server returned an invalid status");
                    return ExitCode::Internal;
                }
            };
            super::render_tag_action(&super::TagActionResult {
                status: status as i32,
                detail: result.detail,
                progress: result.progress,
            })
        }
        Some(v1::execute_tag_bump_response::Outcome::Rejected(rejection)) => {
            eprintln!("tag bump: {}", rejection.detail);
            ExitCode::Internal
        }
        None => {
            eprintln!("tag bump: gtl-server returned no tag-bump execution outcome");
            ExitCode::Internal
        }
    }
}

fn prepare_bump(
    level: TagBumpLevel,
    pattern: Option<String>,
    message: String,
    push: bool,
) -> anyhow::Result<(ServerClient, v1::TagBumpPreview)> {
    let repo_path = commands::canonical_working_directory()?;
    let client = ServerClient::connect()?;
    let prepared = client.plan_tag_bump(v1::PlanTagBumpRequest {
        repository_path: repo_path.to_string_lossy().into_owned(),
        level: Some(to_grpc_level(level)),
        message,
        push,
        pattern,
    })?;
    let preview = match prepared.outcome {
        Some(v1::plan_tag_bump_response::Outcome::Ready(preview)) => preview,
        Some(v1::plan_tag_bump_response::Outcome::Rejected(rejection)) => {
            anyhow::bail!(rejection.detail)
        }
        None => anyhow::bail!("gtl-server returned no tag-bump plan outcome"),
    };
    Ok((client, preview))
}

fn to_grpc_level(level: TagBumpLevel) -> v1::TagBumpLevel {
    let kind = match level {
        TagBumpLevel::Slot(index) => v1::tag_bump_level::Kind::SlotFromRight(index),
        TagBumpLevel::Major => {
            v1::tag_bump_level::Kind::Component(v1::SemverComponent::Major as i32)
        }
        TagBumpLevel::Minor => {
            v1::tag_bump_level::Kind::Component(v1::SemverComponent::Minor as i32)
        }
        TagBumpLevel::Patch => {
            v1::tag_bump_level::Kind::Component(v1::SemverComponent::Patch as i32)
        }
    };
    v1::TagBumpLevel { kind: Some(kind) }
}

fn confirmation(preview: &v1::TagBumpPreview) -> anyhow::Result<Dialog> {
    let project = RepositoryRoot::try_new(preview.repository_root.clone().into())?.project_name();
    let push = if preview.push {
        if preview.push_urls.is_empty() {
            "origin (URL unavailable)".to_string()
        } else {
            format!("origin ({})", preview.push_urls.join(", "))
        }
    } else {
        "No".to_string()
    };
    let question = if preview.push {
        format!("Create and push annotated tag {}?", preview.next_tag)
    } else {
        format!("Create annotated tag {}?", preview.next_tag)
    };
    Ok(Dialog::new(
        "Confirm tag bump",
        vec![
            Detail::new("Project", project),
            Detail::new("Base tag", preview.base_tag.as_deref().unwrap_or("None")),
            Detail::new("New tag", &preview.next_tag),
            Detail::new("Push", push),
        ],
        question,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_preview_shows_only_the_project_version_and_publication() {
        let preview = v1::TagBumpPreview {
            repository_root: "/repos/example-project".into(),
            base_tag: Some("v1.2.3".into()),
            next_tag: "v1.2.4".into(),
            push: true,
            push_urls: vec!["git@example.invalid:team/example-project.git".into()],
            message: "private multiline\nmessage".into(),
            ..Default::default()
        };
        assert_eq!(
            confirmation(&preview)
                .unwrap()
                .render("Tag bump preview", false),
            "Tag bump preview\n\n  Project   example-project\n  Base tag  v1.2.3\n  New tag   v1.2.4\n  Push      origin (git@example.invalid:team/example-project.git)"
        );
    }
}
