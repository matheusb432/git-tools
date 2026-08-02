use gtl_application::tags::{ListTagsOk, TagGroup};
use gtl_models::tags::{Tag, TagState};

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
        format!("{} {}{state}{message}", tag.commit_short(), tag.name())
    } else {
        format!("{}{state}{message}", tag.name())
    }
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

#[cfg(test)]
mod tests {
    use gtl_application::tags::{ListTagsOk, TagGroup};
    use gtl_models::tags::{Tag, TagState};

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
