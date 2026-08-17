//! Dispatching and formatting the status of managed repos (`status`,
//! `status --current`, `status --recursive`). Classification lives in the
//! `gtl_application::repositories::get_repository_statuses` slice; only repo-list resolution and
//! terminal formatting stay here.

use std::path::Path;

use gtl_application::repositories::{
    find_repositories, get_repository_statuses, resolve_repository_root,
};
use gtl_infra::git_client::HybridGitClient;
pub use gtl_models::repository::status::StatusResult;
use gtl_models::{
    projects::ProjectRepository,
    repository::traversal::{RepositoryTarget, RepositoryTraversalScope},
};

use self::palette::StatusColorPalette;
use super::{ManagedExit, ManagedOptions, ManagedRun};
mod palette;

pub fn run_status(options: &ManagedOptions) -> ManagedRun<StatusResult> {
    match super::project_catalog::load_projects() {
        Ok(repos) => status_run(
            get_repository_statuses::execute(
                get_repository_statuses::GetRepositoryStatuses {
                    repos: repos.into_iter().map(project_target).collect(),
                },
                &HybridGitClient,
            ),
            options,
        ),
        Err(error) => status_fail(format!("{error:#}")),
    }
}

/// Status of the single repo that contains `dir` (resolved via `git rev-parse
/// --show-toplevel`, so it works from any subdirectory). Fails (exit 2) when `dir`
/// is not inside a git repo.
pub fn run_status_current(dir: &Path, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let top = match resolve_repository_root::execute(
        resolve_repository_root::ResolveRepositoryRoot {
            repo_path: dir.to_path_buf(),
        },
        &HybridGitClient,
    ) {
        Ok(top) => top,
        Err(error) => return status_fail(format!("status: {error:#}")),
    };
    let repo = RepositoryTarget {
        label: gtl_application::shared::repository_name::from_root(&top),
        path: top,
    };
    status_run(
        get_repository_statuses::execute(
            get_repository_statuses::GetRepositoryStatuses { repos: vec![repo] },
            &HybridGitClient,
        ),
        options,
    )
}

/// Status of the repo at `root` plus every nested subrepo beneath it. Linked
/// worktrees (and their subtrees) are skipped — they mirror a repo already
/// reported elsewhere. Fails (exit 2) when no git repo is found under `root`.
pub fn run_status_recursive(root: &Path, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let root = match std::fs::canonicalize(root) {
        Ok(root) => root,
        Err(error) => {
            return status_fail(format!(
                "status: failed to resolve {}: {error}",
                root.display()
            ));
        }
    };
    let discovered = match find_repositories::execute(find_repositories::FindRepositories {
        root: root.clone(),
        scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
    }) {
        Ok(discovered) => discovered,
        Err(error) => return status_fail(format!("status: {error:#}")),
    };
    if discovered.is_empty() {
        return status_fail(format!(
            "status: no git repos found under {}",
            root.display()
        ));
    }

    let repos = discovered;
    status_run(
        get_repository_statuses::execute(
            get_repository_statuses::GetRepositoryStatuses { repos },
            &HybridGitClient,
        ),
        options,
    )
}

fn project_target(repo: ProjectRepository) -> RepositoryTarget {
    RepositoryTarget {
        path: repo.path,
        label: repo.name,
    }
}

fn status_run(results: Vec<StatusResult>, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    match format_status(
        options.output.is_json(),
        options.output.color_enabled(),
        &results,
    ) {
        Ok(stdout) => ManagedRun {
            exit: ManagedExit::Clean,
            results,
            stdout,
            stderr: String::new(),
        },
        Err(error) => status_fail(format!("status: {error:#}")),
    }
}

fn status_fail(message: String) -> ManagedRun<StatusResult> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: message,
    }
}

fn format_status(json: bool, color: bool, results: &[StatusResult]) -> anyhow::Result<String> {
    if json {
        return serde_json::to_string_pretty(results).map_err(Into::into);
    }

    let palette = color
        .then(StatusColorPalette::from_embedded_toml)
        .transpose()?;
    Ok(results
        .iter()
        .map(|result| format_status_line(result, palette.as_ref()))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn format_status_line(result: &StatusResult, palette: Option<&StatusColorPalette>) -> String {
    let detail = result.detail();
    if !result.is_present() {
        return format!(
            "{} (absent) {}",
            result.name(),
            format_bracketed_status(&detail, palette)
        );
    }

    let branch = result.branch_label().unwrap_or("(unknown)");
    format!(
        "{} {} {}",
        result.name(),
        branch,
        format_bracketed_status(&detail, palette)
    )
}

fn format_bracketed_status(detail: &str, palette: Option<&StatusColorPalette>) -> String {
    let Some(palette) = palette else {
        return format!("[{detail}]");
    };

    format!(
        "\x1b[1m{}{}{}\x1b[0m",
        palette.brackets.paint("["),
        colorize_status_detail(detail, palette),
        palette.brackets.paint("]")
    )
}

fn colorize_status_detail(detail: &str, palette: &StatusColorPalette) -> String {
    detail
        .split_whitespace()
        .map(|token| match token {
            "✓" => palette.checkmark.paint(token),
            "!" | "?" | "!?" | "?!" => palette.change_markers.paint(token),
            ahead if ahead.starts_with('⇡') => colorize_ahead(ahead, palette),
            _ => token.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn colorize_ahead(value: &str, palette: &StatusColorPalette) -> String {
    let Some(count) = value.strip_prefix('⇡') else {
        return value.to_string();
    };
    format!("{}{}", palette.ahead_arrow.paint("⇡"), count)
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::{BranchName, CommitCount, GitRefName},
        repository::{
            PathCount,
            status::{StatusChanges, StatusHead, StatusUpstream},
        },
    };

    use super::*;
    use crate::testing::project_name;

    fn branch_status(name: &str, ahead: u64, changes: StatusChanges) -> StatusResult {
        StatusResult::present(
            project_name(name),
            StatusHead::Branch {
                name: BranchName::main(),
                upstream: StatusUpstream::Tracking {
                    reference: GitRefName::try_new("origin/main").unwrap(),
                    ahead: CommitCount::new(ahead),
                },
            },
            changes,
        )
    }

    #[test]
    fn status_formatting_preserves_present_and_absent_rows() {
        let results = [
            branch_status(
                "repo",
                1,
                StatusChanges::from_counts(PathCount::new(1), PathCount::new(1)),
            ),
            StatusResult::absent(project_name("missing")),
        ];

        assert_eq!(
            format_status(false, false, &results).expect("status should format"),
            "repo main [⇡1 !?]\nmissing (absent) [not present]"
        );
    }

    #[test]
    fn status_formatting_colors_brackets_ahead_changes_and_checkmarks() {
        let results = [
            branch_status(
                "dirty",
                1,
                StatusChanges::from_counts(PathCount::new(1), PathCount::new(1)),
            ),
            branch_status("clean", 0, StatusChanges::Clean),
        ];
        let rendered = format_status(false, true, &results).expect("status should format");

        assert!(rendered.contains("\x1b[1m\x1b[38;2;242;133;0m[\x1b[39m"));
        assert!(rendered.contains("\x1b[38;2;242;133;0m⇡\x1b[39m1"));
        assert!(rendered.contains("\x1b[38;2;255;77;77m!?\x1b[39m"));
        assert!(rendered.contains("\x1b[38;2;46;204;113m✓\x1b[39m"));
    }
}
