use gtl_models::paths::RepositoryRoot;
use gtl_wire::v1;

use crate::{
    ExitCode,
    cli::TagBumpLevel,
    commands,
    confirm::{Confirmation, DefaultAnswer, RealConfirm},
    server_client::ServerClient,
};

pub fn run(level: TagBumpLevel, message: String, push: bool, dry: bool, yes: bool) -> ExitCode {
    let (client, preview) = match prepare_bump(level, message, push) {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("tag bump: {}", crate::error_text(&error));
            return ExitCode::Internal;
        }
    };

    let rendered_preview = match render_preview(&preview) {
        Ok(rendered) => rendered,
        Err(error) => {
            eprintln!("tag bump: {}", crate::error_text(&error));
            return ExitCode::Internal;
        }
    };
    println!("{rendered_preview}");
    if dry {
        return ExitCode::Ok;
    }

    let question = if preview.push {
        format!("Create and push annotated tag {}?", preview.next_tag)
    } else {
        format!("Create annotated tag {}?", preview.next_tag)
    };
    match crate::confirm::request(&RealConfirm, yes, &question, DefaultAnswer::Yes) {
        Confirmation::RefuseNonInteractive => {
            eprintln!("tag bump: non-interactive shell; pass --yes to create the displayed tag");
            return ExitCode::Usage;
        }
        Confirmation::Declined => {
            println!("tag bump: aborted; no tag created");
            return ExitCode::Ok;
        }
        Confirmation::Invalid(error) => {
            eprintln!("tag bump: {error}; no tag created");
            return ExitCode::Usage;
        }
        Confirmation::Proceed => {}
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
    if let Err(error) = print_notes(&bumped.notes) {
        eprintln!("tag bump: {}", crate::error_text(&error));
        return ExitCode::Internal;
    }
    match bumped.outcome {
        Some(v1::execute_tag_bump_response::Outcome::Applied(result)) => {
            match v1::TagBumpStatus::try_from(result.status) {
                Ok(
                    v1::TagBumpStatus::Created
                    | v1::TagBumpStatus::NoOp
                    | v1::TagBumpStatus::Pushed,
                ) => ExitCode::Ok,
                Ok(v1::TagBumpStatus::Failed) => ExitCode::Internal,
                Ok(v1::TagBumpStatus::Unspecified) | Err(_) => {
                    eprintln!("tag bump: gtl-server returned an invalid tag-bump status");
                    ExitCode::Internal
                }
            }
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
    message: String,
    push: bool,
) -> anyhow::Result<(ServerClient, v1::TagBumpPreview)> {
    let repo_path = commands::canonical_working_directory()?;
    let client = ServerClient::connect()?;
    let prepared = client.plan_tag_bump(v1::PlanTagBumpRequest {
        repository_path: repo_path.to_string_lossy().into_owned(),
        level: to_grpc_level(level),
        message,
        push,
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

fn to_grpc_level(level: TagBumpLevel) -> i32 {
    (match level {
        TagBumpLevel::Major => v1::TagBumpLevel::Major,
        TagBumpLevel::Minor => v1::TagBumpLevel::Minor,
        TagBumpLevel::Patch => v1::TagBumpLevel::Patch,
    }) as i32
}

fn render_preview(preview: &v1::TagBumpPreview) -> anyhow::Result<String> {
    let message = preview
        .message
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let publish = if preview.push {
        format!("origin (refs/tags/{} only)", preview.next_tag)
    } else {
        "no".to_string()
    };

    Ok(format!(
        "Tag bump review:\n  repository: {} ({})\n  branch: {}\n  base tag: {}\n  bump: {}\n  create: annotated tag {}\n  target: {}\n  message:\n{}\n  push: {}",
        repository_name(&preview.repository_root)?,
        preview.repository_root,
        head_name(preview.head.as_ref())?,
        preview.base_tag,
        level_name(preview.level)?,
        preview.next_tag,
        preview.target_commit_id,
        message,
        publish,
    ))
}

fn repository_name(repo_path: &str) -> anyhow::Result<String> {
    Ok(RepositoryRoot::try_new(repo_path.into())?
        .project_name()
        .to_string())
}

fn level_name(level: i32) -> anyhow::Result<&'static str> {
    match v1::TagBumpLevel::try_from(level) {
        Ok(v1::TagBumpLevel::Major) => Ok("major"),
        Ok(v1::TagBumpLevel::Minor) => Ok("minor"),
        Ok(v1::TagBumpLevel::Patch) => Ok("patch"),
        Ok(v1::TagBumpLevel::Unspecified) | Err(_) => {
            anyhow::bail!("gtl-server returned an invalid tag-bump level")
        }
    }
}

fn head_name(head: Option<&v1::GitHead>) -> anyhow::Result<&str> {
    match head
        .and_then(|head| head.state.as_ref())
        .ok_or_else(|| anyhow::anyhow!("gtl-server returned no tag-bump head state"))?
    {
        v1::git_head::State::Branch(branch) => Ok(branch),
        v1::git_head::State::Detached(_) => Ok("HEAD"),
    }
}

fn print_notes(notes: &[v1::Note]) -> anyhow::Result<()> {
    for note in notes {
        match v1::NoteLevel::try_from(note.level) {
            Ok(v1::NoteLevel::Info) => println!("{}", note.text),
            Ok(v1::NoteLevel::Warning | v1::NoteLevel::Error) => {
                eprintln!("{}", note.text);
            }
            Ok(v1::NoteLevel::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid tag-bump note level")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use gtl_wire::v1;

    use super::render_preview;

    #[test]
    fn preview_names_the_exact_tag_target_message_and_push_ref() {
        let preview = v1::TagBumpPreview {
            repository_root: "/repo/git-tools".into(),
            head: Some(v1::GitHead {
                state: Some(v1::git_head::State::Branch("main".into())),
            }),
            target_commit_id: "0123456789abcdef0123456789abcdef01234567".into(),
            level: v1::TagBumpLevel::Patch as i32,
            base_tag: "v0.30.0".into(),
            next_tag: "v0.30.1".into(),
            message: "release\nnotes".into(),
            push: true,
        };

        assert_eq!(
            render_preview(&preview).unwrap(),
            "Tag bump review:\n  repository: git-tools (/repo/git-tools)\n  branch: main\n  base tag: v0.30.0\n  bump: patch\n  create: annotated tag v0.30.1\n  target: 0123456789abcdef0123456789abcdef01234567\n  message:\n    release\n    notes\n  push: origin (refs/tags/v0.30.1 only)"
        );
    }
}
