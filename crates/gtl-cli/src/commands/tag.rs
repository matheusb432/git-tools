use anyhow::Context as _;
use gtl_models::{
    diffs::{CommitId, CommitIdAbbreviation},
    git::{GitObjectId, TagName},
    tags::{Tag, TagState},
    timestamps::MachineTimestamp,
};
use gtl_wire::v1;

use crate::{cli::TagCommand, server_client::ServerClient};

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
    let repo_path = match super::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tag: {error:#}");
            return crate::ExitCode::Internal;
        }
    };
    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("tag: {error:#}");
            return crate::ExitCode::Internal;
        }
    };
    let repository_path = repo_path.to_string_lossy().into_owned();
    match command {
        Some(TagCommand::Add { tag, message }) => run_add(&client, &repository_path, tag, message),
        Some(TagCommand::Push {
            tag: Some(tag),
            message: Some(message),
            label,
        }) => run_add_and_push(&client, &repository_path, tag, message, label),
        Some(TagCommand::Push {
            tag: Some(tag),
            message: None,
            label: Some(label),
        }) => run_label(&client, &repository_path, tag, label),
        Some(TagCommand::Push {
            tag: None,
            message: None,
            label: None,
        }) => finish_tag_action(client.push_tags(v1::PushTagsRequest { repository_path })),
        Some(TagCommand::Push { tag: None, .. }) => {
            eprintln!("tag: tag push --label requires a <tag> to label");
            crate::ExitCode::Usage
        }
        Some(TagCommand::Push { .. }) => {
            eprintln!("tag: tag push requires both <tag> and <message> when creating a tag");
            crate::ExitCode::Usage
        }
        Some(TagCommand::Ls) | None => finish_tag_list(
            client.list_tags(v1::ListTagsRequest {
                repository_path,
                include_state: state,
            }),
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
    client: &ServerClient,
    repository_path: &str,
    tag: String,
    message: String,
) -> crate::ExitCode {
    let Some(tag) = parse_tag_name(tag) else {
        return crate::ExitCode::Usage;
    };
    finish_tag_action(client.add_tag(v1::AddTagRequest {
        repository_path: repository_path.into(),
        tag: tag.to_string(),
        message,
    }))
}

fn run_add_and_push(
    client: &ServerClient,
    repository_path: &str,
    tag: String,
    message: String,
    label: Option<String>,
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
    finish_tag_action(client.add_and_push_tag(v1::AddAndPushTagRequest {
        repository_path: repository_path.into(),
        tag: tag.to_string(),
        message,
        label: label.map(|label| label.to_string()),
    }))
}

fn run_label(
    client: &ServerClient,
    repository_path: &str,
    tag: String,
    label: String,
) -> crate::ExitCode {
    let Some(tag) = parse_tag_name(tag) else {
        return crate::ExitCode::Usage;
    };
    let Some(label) = parse_tag_name(label) else {
        return crate::ExitCode::Usage;
    };
    finish_tag_action(client.label_tag(v1::LabelTagRequest {
        repository_path: repository_path.into(),
        tag: tag.to_string(),
        label: label.to_string(),
    }))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagGroup {
    Single(Tag),
    Canonical { canonical: Tag, labels: Vec<Tag> },
    MoreThanOneTagHasMessage(Vec<Tag>),
    AllLabels(Vec<Tag>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListTagsOk {
    Listed { groups: Vec<TagGroup> },
    Failed { detail: String },
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

fn finish_tag_list(result: anyhow::Result<v1::ListTagsResponse>, commits: bool) -> crate::ExitCode {
    match result {
        Ok(response) => match list_from_grpc(response) {
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
                eprintln!("tag: {error:#}");
                crate::ExitCode::Internal
            }
        },
        Err(error) => {
            eprintln!("tag: {error:#}");
            crate::ExitCode::Internal
        }
    }
}

fn list_from_grpc(response: v1::ListTagsResponse) -> anyhow::Result<ListTagsOk> {
    match response
        .outcome
        .context("gtl-server omitted the tag-list outcome")?
    {
        v1::list_tags_response::Outcome::Failed(failed) => Ok(ListTagsOk::Failed {
            detail: failed.detail,
        }),
        v1::list_tags_response::Outcome::Listed(listed) => Ok(ListTagsOk::Listed {
            groups: listed
                .groups
                .into_iter()
                .map(tag_group_from_grpc)
                .collect::<anyhow::Result<Vec<_>>>()?,
        }),
    }
}

fn tag_group_from_grpc(group: v1::TagGroup) -> anyhow::Result<TagGroup> {
    match group
        .kind
        .context("gtl-server omitted the tag-group kind")?
    {
        v1::tag_group::Kind::Single(tag) => Ok(TagGroup::Single(tag_from_grpc(tag)?)),
        v1::tag_group::Kind::Canonical(group) => Ok(TagGroup::Canonical {
            canonical: tag_from_grpc(
                group
                    .canonical
                    .context("gtl-server omitted the canonical tag")?,
            )?,
            labels: group
                .labels
                .into_iter()
                .map(tag_from_grpc)
                .collect::<anyhow::Result<Vec<_>>>()?,
        }),
        v1::tag_group::Kind::MultipleAnnotated(tags) => Ok(TagGroup::MoreThanOneTagHasMessage(
            tags.tags
                .into_iter()
                .map(tag_from_grpc)
                .collect::<anyhow::Result<Vec<_>>>()?,
        )),
        v1::tag_group::Kind::AllLabels(tags) => Ok(TagGroup::AllLabels(
            tags.tags
                .into_iter()
                .map(tag_from_grpc)
                .collect::<anyhow::Result<Vec<_>>>()?,
        )),
    }
}

fn tag_from_grpc(tag: v1::Tag) -> anyhow::Result<Tag> {
    let state =
        v1::TagState::try_from(tag.state).context("gtl-server returned an invalid tag state")?;
    let name = TagName::try_new(tag.name).context("gtl-server returned an empty tag name")?;
    let commit = CommitId::try_from(tag.commit_id)
        .context("gtl-server returned an invalid tag commit ID")?;
    let created_at = tag
        .created_at
        .map(|created_at| created_at.parse::<MachineTimestamp>())
        .transpose()
        .context("gtl-server returned an invalid tag timestamp")?;
    let mut tag = if tag.annotated {
        Tag::annotated(
            name,
            GitObjectId::try_new(tag.object_id)
                .context("gtl-server returned an invalid tag object ID")?,
            commit,
            created_at,
            tag.message,
        )
    } else {
        Tag::lightweight(name, commit, created_at)
    };
    match state {
        v1::TagState::NotQueried => {}
        v1::TagState::Local => tag.set_state(TagState::Local),
        v1::TagState::Remote => tag.set_state(TagState::Remote),
        v1::TagState::Unspecified => {
            anyhow::bail!("gtl-server returned an unspecified tag state")
        }
    }
    Ok(tag)
}

fn finish_tag_action(result: anyhow::Result<v1::TagActionResponse>) -> crate::ExitCode {
    match result {
        Ok(outcome) => render_tag_action(&outcome),
        Err(error) => {
            eprintln!("tag: {error:#}");
            crate::ExitCode::Internal
        }
    }
}

fn render_tag_action(outcome: &v1::TagActionResponse) -> crate::ExitCode {
    match v1::TagActionStatus::try_from(outcome.status) {
        Ok(
            v1::TagActionStatus::Created | v1::TagActionStatus::NoOp | v1::TagActionStatus::Pushed,
        ) => {
            if !outcome.detail.is_empty() {
                println!("{}", outcome.detail);
            }
            crate::ExitCode::Ok
        }
        Ok(v1::TagActionStatus::Failed | v1::TagActionStatus::Aborted) => {
            eprintln!("tag: {}", outcome.detail);
            crate::ExitCode::Internal
        }
        Ok(v1::TagActionStatus::Unspecified) | Err(_) => {
            eprintln!("tag: gtl-server returned an invalid tag action status");
            crate::ExitCode::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::tags::{Tag, TagState};

    use super::{ListTagsOk, TagGroup, render_list};
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
