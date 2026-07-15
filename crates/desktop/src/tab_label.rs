use application::diffs::{View, render_merge_diff::DEFAULT_BASE};
use gtl_recipe::{Recipe, RecipeOp, RecipeTarget};

pub(super) fn initial(recipe: &Recipe) -> String {
    if let Some(name) = &recipe.name {
        return name.clone();
    }

    let repo = source_repo_name(recipe);
    match &recipe.op {
        RecipeOp::Diff { target } => initial_diff_label(&repo, target),
        RecipeOp::MergeDiff { base, .. } => {
            let base = base
                .as_deref()
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_BASE);
            format!("{repo}: merge ->{base}")
        }
        RecipeOp::SquashPreview { .. } => format!("{repo}: squash"),
    }
}

pub(super) fn computed(recipe: &Recipe, view: &View) -> String {
    if let Some(name) = &recipe.name {
        return name.clone();
    }

    let repo = &view.repo_name;
    match &recipe.op {
        RecipeOp::Diff { target } => computed_diff_label(repo, target, view),
        RecipeOp::MergeDiff { .. } => merge_label(repo, view),
        RecipeOp::SquashPreview { .. } => {
            format!("{repo}: squash {}", commit_count(view.commits.len()))
        }
    }
}

fn initial_diff_label(repo: &str, target: &RecipeTarget) -> String {
    match target {
        RecipeTarget::Unpushed { .. } => format!("{repo}: diff"),
        RecipeTarget::Base { rev } => format!("{repo}: {rev}->working"),
        RecipeTarget::Range { range, .. } => format!("{repo}: {range}"),
        RecipeTarget::Merge { base, .. } => {
            let base = base.trim();
            if base.is_empty() {
                format!("{repo}: merge")
            } else {
                format!("{repo}: merge ->{base}")
            }
        }
        RecipeTarget::Last { count, .. } => {
            format!("{repo}: last {}", commit_count(count.get() as usize))
        }
    }
}

fn computed_diff_label(repo: &str, target: &RecipeTarget, view: &View) -> String {
    match target {
        RecipeTarget::Unpushed { .. } => {
            format!("{repo}: {}", commit_count(view.commits.len()))
        }
        RecipeTarget::Base { rev } => format!("{repo}: {rev}->working"),
        RecipeTarget::Range { range, .. } => format!("{repo}: {range}"),
        RecipeTarget::Merge { .. } => merge_label(repo, view),
        RecipeTarget::Last { count, .. } => {
            format!("{repo}: last {}", commit_count(count.get() as usize))
        }
    }
}

fn merge_label(repo: &str, view: &View) -> String {
    format!("{repo}: merge {}->{}", view.branch, view.upstream)
}

fn commit_count(count: usize) -> String {
    let suffix = if count == 1 { "commit" } else { "commits" };
    format!("{count} {suffix}")
}

fn source_repo_name(recipe: &Recipe) -> String {
    let cwd = recipe.cwd();
    cwd.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| cwd.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use application::diffs::{Cmd, Foot};
    use domain::diffs::Commit;
    use gtl_recipe::RecipeSource;

    use super::*;

    fn recipe(op: RecipeOp) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo("/repos/git-tools".into()),
            op,
            name: None,
        }
    }

    fn view(commit_count: usize) -> View {
        View {
            exclusions: None,
            repo_name: "git-tools".into(),
            repo_root: "/repos/git-tools".into(),
            branch: "feature".into(),
            upstream: "main".into(),
            commits: (0..commit_count)
                .map(|index| Commit {
                    sha: format!("sha-{index}"),
                    subject: format!("commit {index}"),
                    ..Default::default()
                })
                .collect(),
            files: Vec::new(),
            title: "diff".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
            theme: None,
        }
    }

    #[test]
    fn diff_target_labels_distinguish_symbolic_intent() {
        let cases = [
            (
                RecipeTarget::Base { rev: "v1".into() },
                "git-tools: v1->working",
            ),
            (
                RecipeTarget::Range {
                    range: "v1..v2".into(),
                    pinned: None,
                },
                "git-tools: v1..v2",
            ),
            (
                RecipeTarget::Merge {
                    base: "release".into(),
                    pinned: None,
                },
                "git-tools: merge ->release",
            ),
            (
                RecipeTarget::Last {
                    count: NonZeroU32::new(1).expect("non-zero"),
                    pinned: None,
                },
                "git-tools: last 1 commit",
            ),
        ];

        for (target, expected) in cases {
            assert_eq!(initial(&recipe(RecipeOp::Diff { target })), expected);
        }
    }

    #[test]
    fn blank_merge_base_uses_the_compute_default() {
        let recipe = recipe(RecipeOp::MergeDiff {
            base: Some("  ".into()),
            pinned: None,
        });

        assert_eq!(initial(&recipe), "git-tools: merge ->main");
    }

    #[test]
    fn computed_merge_target_includes_both_branches() {
        let recipe = recipe(RecipeOp::Diff {
            target: RecipeTarget::Merge {
                base: "main".into(),
                pinned: None,
            },
        });

        assert_eq!(
            computed(&recipe, &view(2)),
            "git-tools: merge feature->main"
        );
    }
}
