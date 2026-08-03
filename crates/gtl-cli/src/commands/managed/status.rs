//! Dispatching and formatting the status of managed repos (`status`,
//! `status --current`, `status --recursive`). Classification lives in the
//! `gtl_application::managed::status_repos` slice; only repo-list resolution and
//! terminal formatting stay here.

use std::path::Path;

use gtl_application::managed::status_repos;
use gtl_infra::git_client::HybridGitClient;
pub use gtl_models::managed::status::StatusResult;

use self::palette::StatusColorPalette;
use super::{ManagedExit, ManagedOptions, ManagedRepo, ManagedRun};
mod palette;

pub fn run_status(options: &ManagedOptions) -> ManagedRun<StatusResult> {
    match super::manifest::load_repos(options) {
        Ok(repos) => status_run(
            status_repos::execute(status_repos::StatusRepos { repos }, &HybridGitClient),
            options,
        ),
        Err(error) => status_fail(format!("{error:#}")),
    }
}

/// Status of the single repo that contains `dir` (resolved via `git rev-parse
/// --show-toplevel`, so it works from any subdirectory). Fails (exit 2) when `dir`
/// is not inside a git repo.
pub fn run_status_current(dir: &Path, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let top = match gtl_application::discovery::resolve_repo_top::execute(
        gtl_application::discovery::resolve_repo_top::ResolveRepoTop {
            repo_path: dir.to_path_buf(),
        },
        &HybridGitClient,
    ) {
        Ok(top) => top,
        Err(error) => return status_fail(format!("status: {error:#}")),
    };
    let repo = ManagedRepo {
        name: gtl_application::discovery::rules::repo_name(&top),
        path: top,
        remote: String::new(),
    };
    status_run(
        status_repos::execute(
            status_repos::StatusRepos { repos: vec![repo] },
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
    let discovered = match gtl_application::discovery::find_repos::execute(
        gtl_application::discovery::find_repos::DiscoverRepos {
            root: root.clone(),
            include_worktrees: false,
        },
        &gtl_infra::repo_discovery::WalkdirRepoDiscovery,
    ) {
        Ok(discovered) => discovered,
        Err(error) => return status_fail(format!("status: {error:#}")),
    };
    if discovered.is_empty() {
        return status_fail(format!(
            "status: no git repos found under {}",
            root.display()
        ));
    }

    let repos = discovered
        .into_iter()
        .map(|repo| ManagedRepo {
            name: repo.label,
            path: repo.path,
            remote: String::new(),
        })
        .collect::<Vec<_>>();
    status_run(
        status_repos::execute(status_repos::StatusRepos { repos }, &HybridGitClient),
        options,
    )
}

fn status_run(results: Vec<StatusResult>, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let stdout = format_status(options.json, options.color, &results);
    ManagedRun {
        exit: ManagedExit::Clean,
        results,
        stdout,
        stderr: String::new(),
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

fn format_status(json: bool, color: bool, results: &[StatusResult]) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let palette = color.then(StatusColorPalette::from_embedded_toml);
    results
        .iter()
        .map(|result| format_status_line(result, palette.as_ref()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_status_line(result: &StatusResult, palette: Option<&StatusColorPalette>) -> String {
    if !result.present {
        return format!(
            "{} (absent) {}",
            result.name,
            format_bracketed_status(&result.detail, palette)
        );
    }

    let branch = if result.branch.is_empty() {
        "(unknown)"
    } else {
        result.branch.as_str()
    };
    format!(
        "{} {} {}",
        result.name,
        branch,
        format_bracketed_status(&result.detail, palette)
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
    use super::*;

    fn status_result(name: &str, present: bool, branch: &str, detail: &str) -> StatusResult {
        StatusResult {
            name: name.into(),
            present,
            branch: branch.into(),
            upstream: String::new(),
            ahead: 0,
            dirty: false,
            dirty_count: 0,
            untracked_count: 0,
            state: String::new(),
            detail: detail.into(),
        }
    }

    #[test]
    fn status_formatting_preserves_present_and_absent_rows() {
        let results = [
            status_result("repo", true, "main", "⇡1 !?"),
            status_result("missing", false, "", "not present"),
        ];

        assert_eq!(
            format_status(false, false, &results),
            "repo main [⇡1 !?]\nmissing (absent) [not present]"
        );
    }

    #[test]
    fn status_formatting_colors_brackets_ahead_changes_and_checkmarks() {
        let results = [
            status_result("dirty", true, "main", "⇡1 !?"),
            status_result("clean", true, "main", "✓"),
        ];
        let rendered = format_status(false, true, &results);

        assert!(rendered.contains("\x1b[1m\x1b[38;2;242;133;0m[\x1b[39m"));
        assert!(rendered.contains("\x1b[38;2;242;133;0m⇡\x1b[39m1"));
        assert!(rendered.contains("\x1b[38;2;255;77;77m!?\x1b[39m"));
        assert!(rendered.contains("\x1b[38;2;46;204;113m✓\x1b[39m"));
    }
}
