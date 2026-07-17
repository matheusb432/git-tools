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
