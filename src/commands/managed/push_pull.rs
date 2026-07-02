//! Fanning `push`/`pull` out across every managed repo.

use serde::Serialize;

use super::git_capture::git_capture;
use super::{ManagedExit, ManagedOptions, ManagedRepo, ManagedRun};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PushPullResult {
    pub name: String,
    pub branch: String,
    pub status: String,
    pub detail: String,
}

fn push_pull_exit_code(statuses: &[&str]) -> ManagedExit {
    if statuses.contains(&"fail") {
        return ManagedExit::Fail;
    }
    if statuses.contains(&"warn") {
        return ManagedExit::Warn;
    }
    ManagedExit::Clean
}

pub fn run_push_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    match super::manifest::load_repos(options) {
        Ok(repos) => {
            let results = repos
                .iter()
                .map(|repo| push_one(repo, options.dry))
                .collect::<Vec<_>>();
            let statuses = results
                .iter()
                .map(|result| result.status.as_str())
                .collect::<Vec<_>>();
            let exit = push_pull_exit_code(&statuses);
            let stdout = format_push_pull("push", options.dry, options.json, &results);
            ManagedRun {
                exit,
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

pub fn run_pull_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    match super::manifest::load_repos(options) {
        Ok(repos) => {
            let results = repos
                .iter()
                .map(|repo| pull_one(repo, options.dry))
                .collect::<Vec<_>>();
            let statuses = results
                .iter()
                .map(|result| result.status.as_str())
                .collect::<Vec<_>>();
            let exit = push_pull_exit_code(&statuses);
            let stdout = format_push_pull("pull", options.dry, options.json, &results);
            ManagedRun {
                exit,
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

fn push_one(repo: &ManagedRepo, dry: bool) -> PushPullResult {
    let mut result = push_pull_result(repo, "", "", "");
    if !repo.path.join(".git").exists() {
        result.status = "skip".to_string();
        result.detail = "not present on this machine".to_string();
        return result;
    }

    let branch = match git_capture(&repo.path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(output) if output.success() => output.stdout.trim().to_string(),
        _ => String::new(),
    };
    result.branch = branch.clone();
    if branch.is_empty() || branch == "HEAD" {
        result.status = "warn".to_string();
        result.detail = "detached HEAD - nothing to push".to_string();
        return result;
    }

    match git_capture(&repo.path, &["remote", "get-url", "origin"]) {
        Ok(output) if output.success() => {}
        _ => {
            result.status = "warn".to_string();
            result.detail = "no 'origin' remote".to_string();
            return result;
        }
    }

    let mut args = vec!["push", "origin", branch.as_str()];
    if dry {
        args.push("--dry-run");
    }
    match git_capture(&repo.path, &args) {
        Ok(output) if output.success() => {
            let combined = output.combined();
            if combined.contains("Everything up-to-date") {
                result.status = "up-to-date".to_string();
                result.detail = "up to date".to_string();
            } else {
                result.status = if dry { "would-push" } else { "pushed" }.to_string();
                result.detail = last_non_empty_line(&combined)
                    .unwrap_or("up to date")
                    .to_string();
            }
        }
        Ok(output) => {
            result.status = "fail".to_string();
            let combined = output.combined();
            result.detail = push_failure_detail(&combined);
        }
        Err(error) => {
            result.status = "fail".to_string();
            result.detail = error.to_string();
        }
    }
    result
}

fn pull_one(repo: &ManagedRepo, dry: bool) -> PushPullResult {
    let mut result = push_pull_result(repo, "", "", "");
    if !repo.path.join(".git").exists() {
        result.status = "skip".to_string();
        result.detail = "not present on this machine".to_string();
        return result;
    }

    let branch = match git_capture(&repo.path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(output) if output.success() => output.stdout.trim().to_string(),
        _ => String::new(),
    };
    result.branch = branch.clone();
    if branch.is_empty() || branch == "HEAD" {
        result.status = "warn".to_string();
        result.detail = "detached HEAD - nothing to pull onto".to_string();
        return result;
    }

    match git_capture(&repo.path, &["remote", "get-url", "origin"]) {
        Ok(output) if output.success() => {}
        _ => {
            result.status = "warn".to_string();
            result.detail = "no 'origin' remote".to_string();
            return result;
        }
    }

    match git_capture(&repo.path, &["fetch", "origin"]) {
        Ok(output) if output.success() => {}
        Ok(output) => {
            result.status = "fail".to_string();
            result.detail = format!(
                "fetch failed: {}",
                last_non_empty_line(&output.combined()).unwrap_or("fetch failed")
            );
            return result;
        }
        Err(error) => {
            result.status = "fail".to_string();
            result.detail = format!("fetch failed: {error}");
            return result;
        }
    }

    let remote_branch = format!("refs/remotes/origin/{branch}");
    match git_capture(
        &repo.path,
        &["rev-parse", "--verify", "--quiet", &remote_branch],
    ) {
        Ok(output) if output.success() => {}
        _ => {
            result.status = "warn".to_string();
            result.detail = format!("no '{branch}' branch on origin");
            return result;
        }
    }

    let range = format!("origin/{branch}...{branch}");
    let counts = match git_capture(&repo.path, &["rev-list", "--count", "--left-right", &range]) {
        Ok(output) if output.success() => output.stdout,
        _ => {
            result.status = "fail".to_string();
            result.detail = "rev-list failed".to_string();
            return result;
        }
    };
    let mut parts = counts.split_whitespace();
    let behind = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let ahead = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    if behind == 0 {
        result.status = "up-to-date".to_string();
        result.detail = if ahead > 0 {
            format!("up to date (local ahead by {ahead} - push pending)")
        } else {
            "up to date".to_string()
        };
        return result;
    }
    if ahead > 0 {
        result.status = "fail".to_string();
        result.detail = format!("diverged (ahead {ahead}, behind {behind}) - resolve manually");
        return result;
    }
    if dry {
        result.status = "would-pull".to_string();
        result.detail = format!("behind by {behind} - fast-forward");
        return result;
    }

    let merge_ref = format!("origin/{branch}");
    match git_capture(&repo.path, &["merge", "--ff-only", &merge_ref]) {
        Ok(output) if output.success() => {
            result.status = "pulled".to_string();
            result.detail = format!(
                "fast-forwarded {behind} commit{}",
                if behind == 1 { "" } else { "s" }
            );
        }
        Ok(output) => {
            result.status = "fail".to_string();
            let combined = output.combined();
            result.detail = combined
                .lines()
                .find(|line| line.starts_with("error:") || line.starts_with("fatal:"))
                .map(str::trim)
                .unwrap_or("ff merge failed")
                .to_string();
        }
        Err(error) => {
            result.status = "fail".to_string();
            result.detail = error.to_string();
        }
    }
    result
}

fn push_pull_result(
    repo: &ManagedRepo,
    branch: &str,
    status: &str,
    detail: &str,
) -> PushPullResult {
    PushPullResult {
        name: repo.name.clone(),
        branch: branch.to_string(),
        status: status.to_string(),
        detail: detail.to_string(),
    }
}

fn push_failure_detail(output: &str) -> String {
    output
        .lines()
        .find(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("!")
                || trimmed.starts_with("error:")
                || trimmed.starts_with("fatal:")
        })
        .map(str::trim)
        .or_else(|| last_non_empty_line(output))
        .unwrap_or("push failed")
        .to_string()
}

pub(super) fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn format_push_pull(label: &str, dry: bool, json: bool, results: &[PushPullResult]) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let mut out = String::new();
    for result in results {
        let verb = if dry {
            format!("dry {label}")
        } else {
            label.to_string()
        };
        let arrow = if label == "pull" { "<-" } else { "->" };
        out.push_str(&format!("{verb} {arrow} {}\n", result.name));
    }
    out.push('\n');
    out.push_str(&format!(
        "{:<30} {:<18} {:<12} {}\n",
        "REPO", "BRANCH", "STATUS", "DETAIL"
    ));
    for result in results {
        out.push_str(&format!(
            "{:<30} {:<18} {:<12} {}\n",
            result.name, result.branch, result.status, result.detail
        ));
    }
    let fail = results
        .iter()
        .filter(|result| result.status == "fail")
        .count();
    let warn = results
        .iter()
        .filter(|result| result.status == "warn")
        .count();
    let statuses = results
        .iter()
        .map(|result| result.status.as_str())
        .collect::<Vec<_>>();
    out.push_str(&format!(
        "\nexit {}  -  {} repos: {} fail, {} warn",
        push_pull_exit_code(&statuses).code(),
        results.len(),
        fail,
        warn
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::managed::test_support::{ManagedFixture, git, git_out};

    #[test]
    fn push_and_pull_exit_codes_preserve_fail_warn_clean_precedence() {
        assert_eq!(
            push_pull_exit_code(&["skip", "up-to-date"]),
            ManagedExit::Clean
        );
        assert_eq!(
            push_pull_exit_code(&["warn", "up-to-date"]),
            ManagedExit::Warn
        );
        assert_eq!(push_pull_exit_code(&["warn", "fail"]), ManagedExit::Fail);
    }

    #[test]
    fn pull_all_dry_reports_would_pull_without_moving_head() {
        let fixture = ManagedFixture::new("pull-dry");
        let (origin, seed) = fixture.origin_with_seed();
        let local = fixture.clone_repo(&origin, "repo");
        fixture.write_manifest(&[("repo", "")]);
        fixture.write_file_in(&seed, "remote.txt", "new\n");
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-m", "remote change"]);
        git(&seed, &["push", "origin", "main"]);
        let before = git_out(&local, &["rev-parse", "HEAD"]);

        let run = run_pull_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: true,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].status, "would-pull");
        assert_eq!(git_out(&local, &["rev-parse", "HEAD"]), before);
    }

    #[test]
    fn pull_all_fails_divergence_without_merging() {
        let fixture = ManagedFixture::new("pull-diverged");
        let (origin, seed) = fixture.origin_with_seed();
        let local = fixture.clone_repo(&origin, "repo");
        fixture.write_manifest(&[("repo", "")]);
        fixture.write_file_in(&seed, "remote.txt", "remote\n");
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-m", "remote change"]);
        git(&seed, &["push", "origin", "main"]);
        fixture.write_file_in(&local, "local.txt", "local\n");
        git(&local, &["add", "-A"]);
        git(&local, &["commit", "-m", "local change"]);
        let before = git_out(&local, &["rev-parse", "HEAD"]);

        let run = run_pull_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Fail);
        assert_eq!(run.results[0].status, "fail");
        assert_eq!(git_out(&local, &["rev-parse", "HEAD"]), before);
    }

    #[test]
    fn push_all_dry_reports_would_push_without_moving_remote() {
        let fixture = ManagedFixture::new("push-dry");
        let (origin, _seed) = fixture.origin_with_seed();
        let local = fixture.clone_repo(&origin, "repo");
        fixture.write_manifest(&[("repo", "")]);
        let remote_before = git_out(&origin, &["rev-parse", "refs/heads/main"]);
        fixture.write_file_in(&local, "local.txt", "local\n");
        git(&local, &["add", "-A"]);
        git(&local, &["commit", "-m", "local change"]);

        let run = run_push_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: true,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].status, "would-push");
        assert_eq!(
            git_out(&origin, &["rev-parse", "refs/heads/main"]),
            remote_before
        );
    }
}
