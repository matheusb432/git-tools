use std::path::Path;

use crate::commands::git_runner::GitRunner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Listed,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeResult {
    pub status: Status,
    pub detail: String,
}

impl WorktreeResult {
    fn new(status: Status, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }
}

pub fn base(runner: &impl GitRunner, repo: &Path) -> WorktreeResult {
    let worktrees = match load_worktrees(runner, repo) {
        Ok(worktrees) => worktrees,
        Err(detail) => return WorktreeResult::new(Status::Fail, detail),
    };

    match worktrees.first() {
        Some(worktree) => WorktreeResult::new(Status::Listed, worktree.path.clone()),
        None => WorktreeResult::new(Status::Fail, "git returned no worktrees"),
    }
}

pub fn list(runner: &impl GitRunner, repo: &Path) -> WorktreeResult {
    let worktrees = match load_worktrees(runner, repo) {
        Ok(worktrees) => worktrees,
        Err(detail) => return WorktreeResult::new(Status::Fail, detail),
    };

    if worktrees.is_empty() {
        return WorktreeResult::new(Status::Fail, "git returned no worktrees");
    }

    WorktreeResult::new(Status::Listed, format_worktrees(&worktrees))
}

fn load_worktrees(runner: &impl GitRunner, repo: &Path) -> Result<Vec<Worktree>, String> {
    match runner.run(repo, &["worktree", "list", "--porcelain"]) {
        Ok(output) if output.exit_code == 0 => Ok(parse_worktrees(&output.stdout)),
        Ok(output) => Err(format!(
            "git worktree list failed (exit {})",
            output.exit_code
        )),
        Err(error) => Err(error.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Worktree {
    path: String,
    head: String,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: Option<String>,
    prunable: Option<String>,
}

impl Worktree {
    fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            head: String::new(),
            branch: None,
            detached: false,
            bare: false,
            locked: None,
            prunable: None,
        }
    }

    fn branch_label(&self) -> String {
        if let Some(branch) = &self.branch {
            branch.clone()
        } else if self.detached {
            "detached".to_string()
        } else {
            "-".to_string()
        }
    }

    fn short_head(&self) -> String {
        if self.head.is_empty() {
            "-".to_string()
        } else {
            self.head.chars().take(7).collect()
        }
    }

    fn details(&self) -> String {
        let mut details = Vec::new();
        if self.bare {
            details.push("bare".to_string());
        }
        if let Some(reason) = &self.locked {
            details.push(detail_with_reason("locked", reason));
        }
        if let Some(reason) = &self.prunable {
            details.push(detail_with_reason("prunable", reason));
        }
        details.join(", ")
    }
}

fn detail_with_reason(label: &str, reason: &str) -> String {
    if reason.is_empty() {
        label.to_string()
    } else {
        format!("{label}: {reason}")
    }
}

fn parse_worktrees(raw: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<Worktree> = None;

    for line in raw.lines() {
        if line.is_empty() {
            if let Some(worktree) = current.take() {
                worktrees.push(worktree);
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(worktree) = current.replace(Worktree::new(path)) {
                worktrees.push(worktree);
            }
            continue;
        }

        let Some(worktree) = current.as_mut() else {
            continue;
        };

        if let Some(head) = line.strip_prefix("HEAD ") {
            worktree.head = head.to_string();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            worktree.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if line == "detached" {
            worktree.detached = true;
        } else if line == "bare" {
            worktree.bare = true;
        } else if let Some(reason) = line.strip_prefix("locked") {
            worktree.locked = Some(reason.trim_start().to_string());
        } else if let Some(reason) = line.strip_prefix("prunable") {
            worktree.prunable = Some(reason.trim_start().to_string());
        }
    }

    if let Some(worktree) = current {
        worktrees.push(worktree);
    }

    worktrees
}

fn format_worktrees(worktrees: &[Worktree]) -> String {
    let rows = worktrees
        .iter()
        .enumerate()
        .map(|(index, worktree)| WorktreeRow {
            path: worktree.path.clone(),
            branch: worktree.branch_label(),
            head: worktree.short_head(),
            state: if index == 0 { "primary" } else { "linked" }.to_string(),
            details: worktree.details(),
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

    #[test]
    fn parse_worktrees_reads_branch_detached_and_metadata() {
        let raw = concat!(
            "worktree /repo\n",
            "HEAD 123456789abcdef\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree /linked\n",
            "HEAD abcdef123456789\n",
            "detached\n",
            "locked maintenance\n",
            "prunable gone\n",
            "\n",
        );

        let worktrees = parse_worktrees(raw);

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, "/repo");
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].branch_label(), "detached");
        assert_eq!(worktrees[1].locked.as_deref(), Some("maintenance"));
        assert_eq!(worktrees[1].prunable.as_deref(), Some("gone"));
    }

    #[test]
    fn format_worktrees_keeps_git_fields_in_a_table() {
        let worktrees = vec![
            Worktree {
                path: "/repo".to_string(),
                head: "123456789abcdef".to_string(),
                branch: Some("main".to_string()),
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            },
            Worktree {
                path: "/linked".to_string(),
                head: "abcdef123456789".to_string(),
                branch: Some("feature/wk".to_string()),
                detached: false,
                bare: true,
                locked: Some("maintenance".to_string()),
                prunable: None,
            },
        ];

        let table = format_worktrees(&worktrees);

        assert!(table.contains("PATH"));
        assert!(table.contains("BRANCH"));
        assert!(table.contains("HEAD"));
        assert!(table.contains("STATE"));
        assert!(table.contains("DETAILS"));
        assert!(table.contains("/repo"));
        assert!(table.contains("main"));
        assert!(table.contains("1234567"));
        assert!(table.contains("primary"));
        assert!(table.contains("/linked"));
        assert!(table.contains("feature/wk"));
        assert!(table.contains("abcdef1"));
        assert!(table.contains("linked"));
        assert!(table.contains("bare, locked: maintenance"));
    }
}
