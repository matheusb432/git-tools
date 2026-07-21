use application::tags::{TagGroup, TagList};
use domain::tags::{Tag, TagState};

pub fn render_list(list: &TagList, commits: bool) -> String {
    let groups = match list {
        TagList::Listed { groups } => groups,
        TagList::Failed { detail } => return detail.clone(),
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
            let mut lines = vec![first.commit_short().to_string()];
            lines.extend(tags.iter().map(render_label));
            lines
        }
    }
}

fn render_tag(tag: &Tag, commits: bool) -> String {
    let state = render_state(tag.state());
    let message = render_message(tag);
    if commits {
        format!("{} {} [{state}]{message}", tag.commit_short(), tag.name())
    } else {
        format!("{} [{state}]{message}", tag.name())
    }
}

fn render_label(tag: &Tag) -> String {
    format!(
        "  - {} [{}]{}",
        tag.name(),
        render_state(tag.state()),
        render_message(tag)
    )
}

fn render_state(state: TagState) -> &'static str {
    match state {
        TagState::Local => "local",
        TagState::Remote => "remote",
    }
}

fn render_message(tag: &Tag) -> String {
    tag.message()
        .map_or_else(String::new, |message| format!("  {message}"))
}

#[cfg(test)]
mod tests {
    use application::tags::{TagGroup, TagList};
    use domain::tags::Tag;

    use super::render_list;

    fn annotated(name: &str, commit_short: &str, message: &str) -> Tag {
        Tag::annotated(
            name.into(),
            format!("{name}-object"),
            format!("{commit_short}-full"),
            commit_short.into(),
            Some(100),
            Some(message.into()),
        )
    }

    fn lightweight(name: &str, commit_short: &str) -> Tag {
        Tag::lightweight(
            name.into(),
            format!("{commit_short}-full"),
            commit_short.into(),
            Some(110),
        )
    }

    #[test]
    fn tag_list_rendering_preserves_state_messages_labels_and_commit_columns() {
        let mut remote = annotated("v1.0.0", "abc1234", "release");
        remote.mark_remote();
        let canonical = annotated("v1.1.0", "def5678", "next");
        let label = lightweight("stable", "def5678");
        let list = TagList::Listed {
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
}
