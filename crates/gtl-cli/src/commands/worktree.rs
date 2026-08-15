use gtl_models::{diffs::CommitIdAbbreviation, worktrees::Worktree};

pub(crate) fn render_list(worktrees: &[Worktree]) -> String {
    let rows = worktrees
        .iter()
        .enumerate()
        .map(|(index, worktree)| WorktreeRow {
            path: worktree.path.clone(),
            branch: branch_label(worktree),
            head: short_head(worktree),
            state: if index == 0 { "primary" } else { "linked" }.to_string(),
            details: details(worktree),
        })
        .collect::<Vec<_>>();

    let path_width = column_width("PATH", rows.iter().map(|row| row.path.as_str()));
    let branch_width = column_width("BRANCH", rows.iter().map(|row| row.branch.as_str()));
    let head_width = column_width("HEAD", rows.iter().map(|row| row.head.as_str()));
    let state_width = column_width("STATE", rows.iter().map(|row| row.state.as_str()));

    let mut lines = vec![format!(
        "{:<path_width$}  {:<branch_width$}  {:<head_width$}  {:<state_width$}  DETAILS",
        "PATH", "BRANCH", "HEAD", "STATE"
    )];

    lines.extend(rows.into_iter().map(|row| {
        format!(
            "{:<path_width$}  {:<branch_width$}  {:<head_width$}  {:<state_width$}  {}",
            row.path, row.branch, row.head, row.state, row.details
        )
    }));

    lines.join("\n")
}

fn branch_label(worktree: &Worktree) -> String {
    if let Some(branch) = &worktree.branch {
        branch.clone()
    } else if worktree.detached {
        "detached".to_string()
    } else {
        "-".to_string()
    }
}

fn short_head(worktree: &Worktree) -> String {
    worktree
        .id
        .abbreviated(CommitIdAbbreviation::SevenCharacters)
}

fn details(worktree: &Worktree) -> String {
    let mut details = Vec::new();
    if worktree.bare {
        details.push("bare".to_string());
    }
    if let Some(reason) = &worktree.locked {
        details.push(detail_with_reason("locked", reason));
    }
    if let Some(reason) = &worktree.prunable {
        details.push(detail_with_reason("prunable", reason));
    }
    details.join(", ")
}

fn detail_with_reason(label: &str, reason: &str) -> String {
    if reason.is_empty() {
        label.to_string()
    } else {
        format!("{label}: {reason}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorktreeRow {
    path: String,
    branch: String,
    head: String,
    state: String,
    details: String,
}

fn column_width<'src>(header: &str, values: impl Iterator<Item = &'src str>) -> usize {
    values
        .map(str::len)
        .chain([header.len()])
        .max()
        .unwrap_or(header.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::commit_id;

    #[test]
    fn render_list_keeps_git_fields_in_the_aligned_table() {
        let worktrees = vec![
            Worktree {
                path: "/repo".to_string(),
                id: commit_id("123456789abcdef"),
                branch: Some("main".to_string()),
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            },
            Worktree {
                path: "/linked".to_string(),
                id: commit_id("abcdef123456789"),
                branch: Some("feature/worktree".to_string()),
                detached: false,
                bare: true,
                locked: Some("maintenance".to_string()),
                prunable: None,
            },
        ];

        assert_eq!(
            render_list(&worktrees),
            concat!(
                "PATH     BRANCH            HEAD     STATE    DETAILS\n",
                "/repo    main              1234567  primary  \n",
                "/linked  feature/worktree  abcdef1  linked   bare, locked: maintenance",
            )
        );
    }
}
