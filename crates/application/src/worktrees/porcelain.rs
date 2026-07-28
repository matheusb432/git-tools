//! Pure parsing for `git worktree list --porcelain` output.

use domain::worktrees::Worktree;

pub(crate) fn parse(raw: &str) -> Vec<Worktree> {
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
            if let Some(worktree) = current.replace(Worktree {
                path: path.to_string(),
                head: String::new(),
                branch: None,
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            }) {
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

#[cfg(test)]
mod tests {
    use domain::worktrees::Worktree;

    use super::parse;

    #[test]
    fn preserves_every_worktree_value() {
        assert_eq!(
            parse(concat!(
                "worktree /repo\nHEAD 123456789abcdef\nbranch refs/heads/main\n\n",
                "worktree /linked\nHEAD abcdef123456789\ndetached\nbare\n",
                "locked maintenance\nprunable gone\n\n",
            )),
            vec![
                Worktree {
                    path: "/repo".into(),
                    head: "123456789abcdef".into(),
                    branch: Some("main".into()),
                    detached: false,
                    bare: false,
                    locked: None,
                    prunable: None,
                },
                Worktree {
                    path: "/linked".into(),
                    head: "abcdef123456789".into(),
                    branch: None,
                    detached: true,
                    bare: true,
                    locked: Some("maintenance".into()),
                    prunable: Some("gone".into()),
                },
            ]
        );
    }
}
