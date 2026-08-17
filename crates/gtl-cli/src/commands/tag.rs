use gtl_application::tags::{
    ListTagsOk, TagGroup,
    add_and_push_tag::{self, AddAndPushTag},
    add_tag::{self, AddTag},
    label_tag::{self, LabelTag},
};
use gtl_models::{
    diffs::CommitIdAbbreviation,
    git::TagName,
    paths::RepositoryRoot,
    tags::{Tag, TagState},
};

use crate::cli::TagCommand;

pub mod bump;

pub fn run(command: Option<TagCommand>, commits: bool, state: bool) -> crate::ExitCode {
    match command {
        Some(TagCommand::Bump {
            level,
            message,
            push,
            dry,
            yes,
        }) => bump::run(level, message, push, dry, yes),
        other => run_non_bump(other, commits, state),
    }
}

fn run_non_bump(command: Option<TagCommand>, commits: bool, state: bool) -> crate::ExitCode {
    use gtl_application::tags::{
        list_tags::{self, ListTags},
        push_tags::{self, PushTags},
    };

    let repo_path = match super::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tag: {error:#}");
            return crate::ExitCode::Internal;
        }
    };
    let git = gtl_infra::git_client::HybridGitClient;
    let repo_path = match gtl_application::ports::GitClient::top_level(&git, &repo_path) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tag: {error:#}");
            return crate::ExitCode::Internal;
        }
    };
    match command {
        Some(TagCommand::Add { tag, message }) => run_add(repo_path, tag, message, git),
        Some(TagCommand::Push {
            tag: Some(tag),
            message: Some(message),
            label,
        }) => run_add_and_push(repo_path, tag, message, label, git),
        Some(TagCommand::Push {
            tag: Some(tag),
            message: None,
            label: Some(label),
        }) => run_label(repo_path, tag, label, git),
        Some(TagCommand::Push {
            tag: None,
            message: None,
            label: None,
        }) => finish_tag_action(push_tags::execute(PushTags { repo_path }, &git)),
        Some(TagCommand::Push { tag: None, .. }) => {
            eprintln!("tag: tag push --label requires a <tag> to label");
            crate::ExitCode::Usage
        }
        Some(TagCommand::Push { .. }) => {
            eprintln!("tag: tag push requires both <tag> and <message> when creating a tag");
            crate::ExitCode::Usage
        }
        Some(TagCommand::Ls) | None => finish_tag_list(
            list_tags::execute(
                ListTags {
                    repo_path,
                    include_state: state,
                },
                &git,
            ),
            commits,
        ),
        Some(TagCommand::Bump {
            level,
            message,
            push,
            dry,
            yes,
        }) => bump::run(level, message, push, dry, yes),
    }
}

fn run_add(
    repo_path: RepositoryRoot,
    tag: String,
    message: String,
    git: gtl_infra::git_client::HybridGitClient,
) -> crate::ExitCode {
    let Some(tag) = parse_tag_name(tag) else {
        return crate::ExitCode::Usage;
    };
    finish_tag_action(add_tag::execute(
        AddTag {
            repo_path,
            tag,
            message,
        },
        &git,
    ))
}

fn run_add_and_push(
    repo_path: RepositoryRoot,
    tag: String,
    message: String,
    label: Option<String>,
    git: gtl_infra::git_client::HybridGitClient,
) -> crate::ExitCode {
    let Some(tag) = parse_tag_name(tag) else {
        return crate::ExitCode::Usage;
    };
    let label = match label {
        Some(label) => {
            let Some(label) = parse_tag_name(label) else {
                return crate::ExitCode::Usage;
            };
            Some(label)
        }
        None => None,
    };
    finish_tag_action(add_and_push_tag::execute(
        AddAndPushTag {
            repo_path,
            tag,
            message,
            label,
        },
        &git,
    ))
}

fn run_label(
    repo_path: RepositoryRoot,
    tag: String,
    label: String,
    git: gtl_infra::git_client::HybridGitClient,
) -> crate::ExitCode {
    let Some(tag) = parse_tag_name(tag) else {
        return crate::ExitCode::Usage;
    };
    let Some(label) = parse_tag_name(label) else {
        return crate::ExitCode::Usage;
    };
    finish_tag_action(label_tag::execute(
        LabelTag {
            repo_path,
            tag,
            label,
        },
        &git,
    ))
}

fn parse_tag_name(raw: String) -> Option<TagName> {
    match TagName::try_new(raw) {
        Ok(tag) => Some(tag),
        Err(error) => {
            eprintln!("tag: invalid tag name: {error}");
            None
        }
    }
}

pub fn render_list(list: &ListTagsOk, commits: bool) -> String {
    let groups = match list {
        ListTagsOk::Listed { groups } => groups,
        ListTagsOk::Failed { detail } => return detail.clone(),
    };

    groups
        .iter()
        .flat_map(|group| render_group(group, commits))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_group(group: &TagGroup, commits: bool) -> Vec<String> {
    match group {
        TagGroup::Single(tag) => vec![render_tag(tag, commits)],
        TagGroup::Canonical { canonical, labels } => {
            let mut lines = vec![render_tag(canonical, commits)];
            lines.extend(labels.iter().map(render_label));
            lines
        }
        TagGroup::MoreThanOneTagHasMessage(tags) | TagGroup::AllLabels(tags) => {
            let Some(first) = tags.first() else {
                return Vec::new();
            };
            let mut lines = vec![abbreviated_commit(first)];
            lines.extend(tags.iter().map(render_label));
            lines
        }
    }
}

fn render_tag(tag: &Tag, commits: bool) -> String {
    let state = render_state(tag.state());
    let message = render_message(tag);
    if commits {
        format!("{} {}{state}{message}", abbreviated_commit(tag), tag.name())
    } else {
        format!("{}{state}{message}", tag.name())
    }
}

fn abbreviated_commit(tag: &Tag) -> String {
    tag.commit()
        .abbreviated(CommitIdAbbreviation::SevenCharacters)
}

fn render_label(tag: &Tag) -> String {
    format!(
        "  - {}{}{}",
        tag.name(),
        render_state(tag.state()),
        render_message(tag)
    )
}

fn render_state(state: Option<TagState>) -> &'static str {
    match state {
        None => "",
        Some(TagState::Local) => " [local]",
        Some(TagState::Remote) => " [remote]",
    }
}

fn render_message(tag: &Tag) -> String {
    tag.message()
        .map_or_else(String::new, |message| format!("  {message}"))
}

fn finish_tag_list<E>(result: Result<ListTagsOk, E>, commits: bool) -> crate::ExitCode
where
    E: std::fmt::Display,
{
    match result {
        Ok(list @ ListTagsOk::Listed { .. }) => {
            let detail = render_list(&list, commits);
            if !detail.is_empty() {
                println!("{detail}");
            }
            crate::ExitCode::Ok
        }
        Ok(ListTagsOk::Failed { detail }) => {
            eprintln!("tag: {detail}");
            crate::ExitCode::Internal
        }
        Err(error) => {
            eprintln!("tag: {error}");
            crate::ExitCode::Internal
        }
    }
}

fn finish_tag_action<E>(
    result: Result<gtl_application::tags::TagActionOutcome, E>,
) -> crate::ExitCode
where
    E: std::fmt::Display,
{
    match result {
        Ok(outcome) => render_tag_action(&outcome),
        Err(error) => {
            eprintln!("tag: {error}");
            crate::ExitCode::Internal
        }
    }
}

fn render_tag_action(outcome: &gtl_application::tags::TagActionOutcome) -> crate::ExitCode {
    use gtl_application::tags::TagActionStatus;

    match outcome.status() {
        TagActionStatus::Created | TagActionStatus::Noop | TagActionStatus::Pushed => {
            if !outcome.detail().is_empty() {
                println!("{}", outcome.detail());
            }
            crate::ExitCode::Ok
        }
        TagActionStatus::Failed => {
            eprintln!("tag: {}", outcome.detail());
            crate::ExitCode::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::tags::{ListTagsOk, TagGroup};
    use gtl_models::tags::{Tag, TagState};

    use super::render_list;
    use crate::testing::{commit_id, git_object_id, tag_name};

    fn annotated(name: &str, commit_prefix: &str, message: &str) -> Tag {
        Tag::annotated(
            tag_name(name),
            git_object_id("f"),
            commit_id(commit_prefix),
            Some(
                gtl_models::timestamps::MachineTimestamp::from_unix_seconds(100)
                    .expect("fixture tag timestamp is in range"),
            ),
            Some(message.into()),
        )
    }

    fn lightweight(name: &str, commit_prefix: &str) -> Tag {
        Tag::lightweight(
            tag_name(name),
            commit_id(commit_prefix),
            Some(
                gtl_models::timestamps::MachineTimestamp::from_unix_seconds(110)
                    .expect("fixture tag timestamp is in range"),
            ),
        )
    }

    #[test]
    fn tag_list_rendering_preserves_state_messages_labels_and_commit_columns() {
        let mut remote = annotated("v1.0.0", "abc1234", "release");
        remote.set_state(TagState::Remote);
        let mut canonical = annotated("v1.1.0", "def5678", "next");
        canonical.set_state(TagState::Local);
        let mut label = lightweight("stable", "def5678");
        label.set_state(TagState::Local);
        let list = ListTagsOk::Listed {
            groups: vec![
                TagGroup::Single(remote),
                TagGroup::Canonical {
                    canonical,
                    labels: vec![label],
                },
            ],
        };

        assert_eq!(
            render_list(&list, false),
            "v1.0.0 [remote]  release\nv1.1.0 [local]  next\n  - stable [local]"
        );
        assert_eq!(
            render_list(&list, true),
            "abc1234 v1.0.0 [remote]  release\ndef5678 v1.1.0 [local]  next\n  - stable [local]"
        );
    }

    #[test]
    fn tags_without_a_queried_state_render_no_state_column() {
        let list = ListTagsOk::Listed {
            groups: vec![TagGroup::Canonical {
                canonical: annotated("v1.1.0", "def5678", "next"),
                labels: vec![lightweight("stable", "def5678")],
            }],
        };

        assert_eq!(render_list(&list, false), "v1.1.0  next\n  - stable");
    }
}
