use std::path::Path;

use gtl_wire::{
    envelope::Outcome,
    tags::{BumpTagRequest, DryRunTagBumpRequest, TagBumpLevelDto, TagBumpPreview},
};

use crate::{
    ExitCode,
    cli::TagBumpLevel,
    client::HttpClient,
    commands,
    confirm::{Confirmation, DefaultAnswer, RealConfirm},
};

pub fn run(level: TagBumpLevel, message: String, push: bool, dry: bool, yes: bool) -> ExitCode {
    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tag bump: {error:#}");
            return ExitCode::Internal;
        }
    };
    let client = match HttpClient::ensure_daemon() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("tag bump: {error:#}");
            return ExitCode::Internal;
        }
    };
    let level = to_level_dto(level);
    let prepared = match client.dry_run_tag_bump(&DryRunTagBumpRequest {
        repo_path: repo_path.to_string_lossy().into_owned(),
        level,
        message,
        push,
    }) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("tag bump: {error:#}");
            return ExitCode::Internal;
        }
    };
    if prepared.outcome != Outcome::Ok {
        eprintln!("tag bump: {}", commands::error_text(&prepared.notes));
        return ExitCode::Internal;
    }
    let Some(preview) = prepared.data else {
        eprintln!("tag bump: daemon returned no preview data");
        return ExitCode::Internal;
    };

    println!("{}", render_preview(&preview));
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

    let bumped = match client.bump_tag(&BumpTagRequest { preview }) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("tag bump: {error:#}");
            return ExitCode::Internal;
        }
    };
    commands::print_wire_notes(&bumped.notes);
    if bumped.outcome == Outcome::Ok {
        ExitCode::Ok
    } else {
        eprintln!("tag bump: {}", commands::error_text(&bumped.notes));
        ExitCode::Internal
    }
}

fn to_level_dto(level: TagBumpLevel) -> TagBumpLevelDto {
    match level {
        TagBumpLevel::Major => TagBumpLevelDto::Major,
        TagBumpLevel::Minor => TagBumpLevelDto::Minor,
        TagBumpLevel::Patch => TagBumpLevelDto::Patch,
    }
}

fn render_preview(preview: &TagBumpPreview) -> String {
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

    format!(
        "Tag bump review:\n  repository: {} ({})\n  branch: {}\n  base tag: {}\n  bump: {}\n  create: annotated tag {}\n  target: {}\n  message:\n{}\n  push: {}",
        repository_name(&preview.repo_path),
        preview.repo_path,
        preview.branch,
        preview.base_tag,
        level_name(preview.level),
        preview.next_tag,
        preview.target_id,
        message,
        publish,
    )
}

fn repository_name(repo_path: &str) -> String {
    Path::new(repo_path).file_name().map_or_else(
        || repo_path.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

const fn level_name(level: TagBumpLevelDto) -> &'static str {
    match level {
        TagBumpLevelDto::Major => "major",
        TagBumpLevelDto::Minor => "minor",
        TagBumpLevelDto::Patch => "patch",
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::tags::{TagBumpLevelDto, TagBumpPreview};

    use super::render_preview;
    use crate::testing::commit_id;

    #[test]
    fn preview_names_the_exact_tag_target_message_and_push_ref() {
        let preview = TagBumpPreview {
            repo_path: "/repo/git-tools".into(),
            branch: "main".into(),
            target_id: commit_id("0123456789abcdef0123456789abcdef01234567"),
            level: TagBumpLevelDto::Patch,
            base_tag: "v0.30.0".into(),
            next_tag: "v0.30.1".into(),
            message: "release\nnotes".into(),
            push: true,
        };

        assert_eq!(
            render_preview(&preview),
            "Tag bump review:\n  repository: git-tools (/repo/git-tools)\n  branch: main\n  base tag: v0.30.0\n  bump: patch\n  create: annotated tag v0.30.1\n  target: 0123456789abcdef0123456789abcdef01234567\n  message:\n    release\n    notes\n  push: origin (refs/tags/v0.30.1 only)"
        );
    }
}
