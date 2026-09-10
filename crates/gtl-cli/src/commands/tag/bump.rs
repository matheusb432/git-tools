use anyhow::Context as _;
use gtl_models::{
    paths::RepositoryRoot,
    tags::{TagSlot, TagTemplate},
};
use gtl_wire::v1;

use crate::{
    ExitCode,
    cli::TagBumpLevel,
    commands,
    confirm::{Confirmation, DefaultAnswer, RealConfirm},
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
        "Tag bump review:\n  repository: {} ({})\n  branch: {}\n  pattern: {} ({})\n  base tag: {}\n  bump: {}\n  create: annotated tag {}\n  target: {}\n  message:\n{}\n  push: {}",
        repository_name(&preview.repository_root)?,
        preview.repository_root,
        head_name(preview.head.as_ref())?,
        preview.pattern,
        preview.template,
        preview.base_tag.as_deref().unwrap_or("none"),
        slot_label(preview)?,
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

fn slot_label(preview: &v1::TagBumpPreview) -> anyhow::Result<String> {
    let template = preview
        .template
        .parse::<TagTemplate>()
        .context("gtl-server returned an invalid tag template")?;
    let slot = TagSlot::try_new(preview.slot_from_right)
        .context("gtl-server returned an invalid tag-bump slot")?;
    Ok(match template.component_at(slot) {
        Some(component) => format!("slot {slot} ({component})"),
        None => format!("slot {slot}"),
    })
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
            pattern: "semver".into(),
            template: "v{major}.{minor}.{patch}".into(),
            slot_from_right: 0,
            base_tag: Some("v0.30.0".into()),
            next_tag: "v0.30.1".into(),
            message: "release\nnotes".into(),
            push: true,
        };

        assert_eq!(
            render_preview(&preview).unwrap(),
            "Tag bump review:\n  repository: git-tools (/repo/git-tools)\n  branch: main\n  pattern: semver (v{major}.{minor}.{patch})\n  base tag: v0.30.0\n  bump: slot 0 (patch)\n  create: annotated tag v0.30.1\n  target: 0123456789abcdef0123456789abcdef01234567\n  message:\n    release\n    notes\n  push: origin (refs/tags/v0.30.1 only)"
        );
    }

    #[test]
    fn preview_names_an_empty_lineage_and_an_anonymous_slot() {
        let preview = v1::TagBumpPreview {
            repository_root: "/repo/sample_project".into(),
            head: Some(v1::GitHead {
                state: Some(v1::git_head::State::Branch("main".into())),
            }),
            target_commit_id: "0123456789abcdef0123456789abcdef01234567".into(),
            pattern: "alpha".into(),
            template: "{major}.{minor}.{patch}-alpha.{n}.{n}".into(),
            slot_from_right: 1,
            base_tag: None,
            next_tag: "0.0.0-alpha.1.0".into(),
            message: "first alpha".into(),
            push: false,
        };

        let rendered = render_preview(&preview).unwrap();
        assert!(rendered.contains("  pattern: alpha ({major}.{minor}.{patch}-alpha.{n}.{n})\n"));
        assert!(rendered.contains("  base tag: none\n"));
        assert!(rendered.contains("  bump: slot 1\n"));
        assert!(rendered.ends_with("  push: no"));
    }
}
