//! Reading and formatting the status of managed repos (`status`, `status --current`,
//! `status --recursive`).

use std::path::{Path, PathBuf};

use serde::Serialize;

use self::palette::StatusColorPalette;
use super::{
    ManagedExit, ManagedOptions, ManagedRepo, ManagedRun, git_capture::git_capture, working_tree,
};
use crate::commands::discover::{discover_git_repos, repo_label};

mod palette;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct StatusResult {
    pub name: String,
    pub present: bool,
    pub branch: String,
    pub upstream: String,
    pub ahead: usize,
    pub dirty: bool,
    pub dirty_count: usize,
    pub untracked_count: usize,
    pub state: String,
    pub detail: String,
}

pub fn run_status(options: &ManagedOptions) -> ManagedRun<StatusResult> {
    match super::manifest::load_repos(options) {
        Ok(repos) => {
            let results = repos.iter().map(status_one).collect::<Vec<_>>();
            let stdout = format_status(options.json, options.color, &results);
            ManagedRun {
                exit: ManagedExit::Clean,
                results,
                stdout,
                stderr: String::new(),
            }
        }
        Err(error) => ManagedRun {
            exit: ManagedExit::Fail,
            results: Vec::new(),
            stdout: String::new(),
            stderr: format!("{error:#}"),
        },
    }
}

/// Status of the single repo that contains `dir` (resolved via `git rev-parse
/// --show-toplevel`, so it works from any subdirectory). Fails (exit 2) when `dir`
/// is not inside a git repo.
pub fn run_status_current(dir: &Path, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let top = match crate::git::top_level(dir) {
        Ok(top) => PathBuf::from(top),
        Err(error) => return status_fail(format!("status: {error:#}")),
    };
    let repo = ManagedRepo {
        name: crate::commands::repo_name(&top),
        path: top,
        remote: String::new(),
    };
    status_run(vec![status_one(&repo)], options)
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
    let repos = match discover_git_repos(&root, false) {
        Ok(repos) => repos,
        Err(error) => return status_fail(format!("status: {error:#}")),
    };
    if repos.is_empty() {
        return status_fail(format!(
            "status: no git repos found under {}",
            root.display()
        ));
    }

    let results = repos
        .iter()
        .map(|path| {
            status_one(&ManagedRepo {
                name: repo_label(&root, path),
                path: path.clone(),
                remote: String::new(),
            })
        })
        .collect::<Vec<_>>();
    status_run(results, options)
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

fn status_one(repo: &ManagedRepo) -> StatusResult {
    let mut result = StatusResult {
        name: repo.name.clone(),
        present: false,
        branch: String::new(),
        upstream: String::new(),
        ahead: 0,
        dirty: false,
        dirty_count: 0,
        untracked_count: 0,
        state: "absent".to_string(),
        detail: "not present".to_string(),
    };

    if !repo.path.join(".git").exists() {
        return result;
    }

    result.present = true;
    result.branch = match git_capture(&repo.path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(output) if output.success() => output.stdout.trim().to_string(),
        _ => String::new(),
    };

    let dirty = working_tree::dirty_state(&repo.path);
    result.untracked_count = dirty
        .files
        .iter()
        .filter(|file| file.status == "??")
        .count();
    result.dirty_count = dirty.files.len().saturating_sub(result.untracked_count);
    result.dirty = result.dirty_count > 0 || result.untracked_count > 0;

    let mut parts = Vec::new();
    if result.branch.is_empty() {
        parts.push("branch-unavailable".to_string());
    } else if result.branch == "HEAD" {
        result.branch = "detached".to_string();
        parts.push("detached".to_string());
    } else {
        result.upstream = match git_capture(
            &repo.path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        ) {
            Ok(output) if output.success() => output.stdout.trim().to_string(),
            _ => String::new(),
        };

        if result.upstream.is_empty() {
            parts.push("no-upstream".to_string());
        } else {
            result.ahead = match git_capture(&repo.path, &["rev-list", "--count", "@{u}..HEAD"]) {
                Ok(output) if output.success() => output.stdout.trim().parse().unwrap_or(0),
                _ => 0,
            };
            if result.ahead > 0 {
                parts.push(format!("⇡{}", result.ahead));
            }
        }
    }

    let change_symbols = format!(
        "{}{}",
        if result.dirty_count > 0 { "!" } else { "" },
        if result.untracked_count > 0 { "?" } else { "" }
    );
    if !change_symbols.is_empty() {
        parts.push(change_symbols);
    }
    if parts.is_empty() {
        parts.push("✓".to_string());
    }

    result.detail = parts.join(" ");
    result.state = if result.detail == "✓" {
        "clean".to_string()
    } else if result.detail.contains("no-upstream")
        || result.detail.contains("detached")
        || result.detail.contains("branch-unavailable")
    {
        "warn".to_string()
    } else {
        "pending".to_string()
    };
    result
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
    use crate::commands::managed::test_support::ManagedFixture;

    fn read_opts() -> ManagedOptions {
        ManagedOptions {
            repos_file: None,
            home_dir: None,
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        }
    }

    #[test]
    fn status_current_reports_the_repo_at_the_given_dir() {
        let fixture = ManagedFixture::new("status-current");
        let repo = fixture.init_repo("repo");

        let run = run_status_current(&repo, &read_opts());

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results.len(), 1);
        assert_eq!(run.results[0].name, "repo");
        assert!(run.results[0].present);
    }

    #[test]
    fn status_current_resolves_repo_from_a_nested_subdirectory() {
        let fixture = ManagedFixture::new("status-current-subdir");
        let repo = fixture.init_repo("repo");
        let nested = repo.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();

        let run = run_status_current(&nested, &read_opts());

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].name, "repo");
    }

    #[test]
    fn status_current_fails_outside_a_git_repo() {
        let fixture = ManagedFixture::new("status-current-norepo");
        let plain = fixture.root.join("plain");
        std::fs::create_dir_all(&plain).unwrap();

        let run = run_status_current(&plain, &read_opts());

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(run.results.is_empty());
        assert!(
            run.stderr.contains("not a git repo"),
            "stderr: {}",
            run.stderr
        );
    }

    #[test]
    fn status_recursive_reports_current_repo_and_nested_subrepos() {
        let fixture = ManagedFixture::new("status-recursive");
        let root = fixture.init_repo("root");
        fixture.init_repo("root/libs/inner");

        let run = run_status_recursive(&root, &read_opts());

        assert_eq!(run.exit, ManagedExit::Clean);
        let names: Vec<&str> = run
            .results
            .iter()
            .map(|result| result.name.as_str())
            .collect();
        assert!(names.contains(&"root"), "names: {names:?}");
        assert!(names.contains(&"libs/inner"), "names: {names:?}");
    }

    #[test]
    fn status_recursive_skips_linked_worktrees() {
        let fixture = ManagedFixture::new("status-recursive-worktrees");
        let root = fixture.init_repo("root");
        fixture.init_repo("root/libs/inner");
        // A linked worktree: `.git` is a file pointing into the repo's worktrees dir.
        let wt = root.join(".worktrees/feature");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            wt.join(".git"),
            "gitdir: /abs/root/.git/worktrees/feature\n",
        )
        .unwrap();

        let run = run_status_recursive(&root, &read_opts());

        let names: Vec<&str> = run
            .results
            .iter()
            .map(|result| result.name.as_str())
            .collect();
        assert!(names.contains(&"root"), "names: {names:?}");
        assert!(names.contains(&"libs/inner"), "names: {names:?}");
        assert!(
            !names.iter().any(|name| name.contains(".worktrees")),
            "linked worktrees should be skipped, names: {names:?}"
        );
    }

    #[test]
    fn status_recursive_fails_when_no_repo_found() {
        let fixture = ManagedFixture::new("status-recursive-empty");
        let empty = fixture.root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();

        let run = run_status_recursive(&empty, &read_opts());

        assert_eq!(run.exit, ManagedExit::Fail);
        assert!(
            run.stderr.contains("no git repos found"),
            "stderr: {}",
            run.stderr
        );
    }
}
