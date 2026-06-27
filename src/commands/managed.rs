use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};

use crate::commands::discover::{discover_git_repos, repo_label};

const STATUS_COLORS_TOML: &str = include_str!("../../config/status-colors.toml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub remote: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedExit {
    Clean,
    Warn,
    Fail,
    Usage,
}

impl ManagedExit {
    pub fn code(self) -> i32 {
        match self {
            ManagedExit::Clean => 0,
            ManagedExit::Warn => 1,
            ManagedExit::Fail => 2,
            ManagedExit::Usage => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedOptions {
    pub repos_file: Option<PathBuf>,
    pub home_dir: Option<PathBuf>,
    pub dry: bool,
    pub json: bool,
    pub color: bool,
    pub message_for_all: Option<String>,
    pub interactive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRun<T> {
    pub exit: ManagedExit,
    pub results: Vec<T>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PushPullResult {
    pub name: String,
    pub branch: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitFile {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitResult {
    pub name: String,
    pub present: bool,
    pub dirty: bool,
    pub files: Vec<CommitFile>,
    pub action: String,
    pub detail: String,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PrunedBranch {
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PruneRepoResult {
    pub name: String,
    pub present: bool,
    /// Branches deleted (or, in dry mode, that would be deleted).
    pub deleted: Vec<PrunedBranch>,
    /// Names of branches that failed to delete.
    pub failed: Vec<String>,
    pub detail: String,
}

/// The `repos.toml` document: an array of `[[repo]]` tables. Only `path` + `remote` are
/// read here; sample_project owns the other fields (`code`/`slug`/`color`).
#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    repo: Vec<RepoEntry>,
}

#[derive(Deserialize)]
struct RepoEntry {
    path: String,
    #[serde(default)]
    remote: String,
    #[serde(default = "default_true")]
    managed: bool,
}

/// serde default for `RepoEntry::managed` — repos are in-scope unless they opt out (CFG-0185).
fn default_true() -> bool {
    true
}

pub fn parse_manifest(raw: &str, home_dir: &Path) -> anyhow::Result<Vec<ManagedRepo>> {
    let manifest: Manifest =
        toml::from_str(raw).context("parsing managed-repos manifest (repos.toml)")?;
    let mut repos = Vec::new();

    for entry in manifest.repo {
        let local = entry.path.trim();
        if local.is_empty() {
            continue;
        }
        if !entry.managed {
            continue;
        }
        repos.push(ManagedRepo {
            name: local.to_string(),
            path: home_dir.join(local),
            remote: entry.remote.trim().to_string(),
        });
    }

    Ok(repos)
}

pub fn resolve_home_dir(override_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = override_dir {
        return dir.to_path_buf();
    }
    home_dir_from_env().unwrap_or_else(|| PathBuf::from("."))
}

pub fn resolve_repos_file(override_file: Option<&Path>) -> anyhow::Result<PathBuf> {
    let env_file = std::env::var_os("GIT_TOOLS_MANAGED_REPOS_FILE").map(PathBuf::from);
    let current_dir = std::env::current_dir()?;
    let home_dir = home_dir_from_env();

    resolve_repos_file_from_sources(
        override_file,
        env_file.as_deref(),
        &current_dir,
        home_dir.as_deref(),
    )
}

fn home_dir_from_env() -> Option<PathBuf> {
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        return Some(PathBuf::from(userprofile));
    }
    std::env::var_os("HOME").map(PathBuf::from)
}

fn resolve_repos_file_from_sources(
    override_file: Option<&Path>,
    env_file: Option<&Path>,
    current_dir: &Path,
    home_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if let Some(file) = override_file {
        return Ok(file.to_path_buf());
    }
    if let Some(file) = env_file {
        return Ok(file.to_path_buf());
    }
    if let Some(found) = find_upward_config(current_dir) {
        return Ok(found);
    }
    if let Some(home_dir) = home_dir {
        let candidate = home_default_repos_file(home_dir);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let home_hint = home_dir
        .map(home_default_repos_file)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "$HOME/self/sample_project/config/provisioning/linux/repos.toml".into());
    Err(anyhow!(
        "managed-repos manifest not found; pass --repos-file, set GIT_TOOLS_MANAGED_REPOS_FILE, run from a sample_project checkout, or install sample_project at {home_hint}"
    ))
}

fn find_upward_config(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir
            .join("config")
            .join("provisioning")
            .join("linux")
            .join("repos.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn home_default_repos_file(home_dir: &Path) -> PathBuf {
    home_dir
        .join("self")
        .join("sample_project")
        .join("config")
        .join("provisioning")
        .join("linux")
        .join("repos.toml")
}

pub fn load_repos(options: &ManagedOptions) -> anyhow::Result<Vec<ManagedRepo>> {
    let repos_file = resolve_repos_file(options.repos_file.as_deref())?;
    let home_dir = resolve_home_dir(options.home_dir.as_deref());
    let raw = std::fs::read_to_string(&repos_file)
        .with_context(|| format!("managed-repos manifest not found: {}", repos_file.display()))?;
    parse_manifest(&raw, &home_dir)
}

pub fn push_pull_exit_code(statuses: &[&str]) -> ManagedExit {
    if statuses.contains(&"fail") {
        return ManagedExit::Fail;
    }
    if statuses.contains(&"warn") {
        return ManagedExit::Warn;
    }
    ManagedExit::Clean
}

pub fn commit_exit_code(actions: &[&str], dry: bool) -> ManagedExit {
    if actions.contains(&"fail") {
        return ManagedExit::Fail;
    }
    if dry && actions.contains(&"would-commit") {
        return ManagedExit::Warn;
    }
    if !dry && actions.contains(&"skipped") {
        return ManagedExit::Warn;
    }
    ManagedExit::Clean
}

pub fn run_push_all(options: &ManagedOptions) -> ManagedRun<PushPullResult> {
    match load_repos(options) {
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
    match load_repos(options) {
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

pub fn run_commit_all(options: &ManagedOptions) -> ManagedRun<CommitResult> {
    if let Some(message) = &options.message_for_all
        && message.trim().is_empty()
    {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit-all: --message-for-all requires a non-empty message".to_string(),
        };
    }

    if !options.dry && options.message_for_all.is_none() && !options.interactive {
        return ManagedRun {
            exit: ManagedExit::Usage,
            results: Vec::new(),
            stdout: String::new(),
            stderr: "commit-all: non-interactive shell; pass --dry or --message-for-all \"msg\""
                .to_string(),
        };
    }

    match load_repos(options) {
        Ok(repos) => {
            let results = repos
                .iter()
                .map(|repo| commit_one(repo, options))
                .collect::<Vec<_>>();
            let actions = results
                .iter()
                .map(|result| result.action.as_str())
                .collect::<Vec<_>>();
            let exit = commit_exit_code(&actions, options.dry);
            let stdout = format_commit(options.dry, options.json, &results);
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

pub fn run_status(options: &ManagedOptions) -> ManagedRun<StatusResult> {
    match load_repos(options) {
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

/// Fan out `prune` over every managed repo. With `options.dry` (set when `--all` is used
/// without `-y`) each repo only reports what *would* be deleted; otherwise branches are
/// deleted. Any non-clean repo (a delete failure or a refusal) maps to [`ManagedExit::Warn`].
pub fn run_prune_all(onto: &str, options: &ManagedOptions) -> ManagedRun<PruneRepoResult> {
    match load_repos(options) {
        Ok(repos) => {
            let runner = crate::commands::squash_local::StdGitRunner;
            let results = repos
                .iter()
                .map(|repo| prune_one(&runner, repo, onto, options.dry))
                .collect::<Vec<_>>();
            let exit = if results.iter().any(|result| !result.failed.is_empty()) {
                ManagedExit::Warn
            } else {
                ManagedExit::Clean
            };
            let stdout = format_prune(onto, options.dry, options.json, &results);
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

fn prune_one(
    runner: &impl crate::commands::squash_local::GitRunner,
    repo: &ManagedRepo,
    onto: &str,
    dry: bool,
) -> PruneRepoResult {
    use crate::commands::prune::{self, PrunePlan};

    let mut result = PruneRepoResult {
        name: repo.name.clone(),
        present: false,
        deleted: Vec::new(),
        failed: Vec::new(),
        detail: String::new(),
    };
    if !repo.path.join(".git").exists() {
        result.detail = "not present on this machine".to_string();
        return result;
    }
    result.present = true;

    match prune::plan(runner, &repo.path, onto) {
        PrunePlan::Refused(detail) | PrunePlan::Nothing(detail) => result.detail = detail,
        PrunePlan::Ready { top, branches, .. } => {
            if dry {
                result.deleted = branches
                    .iter()
                    .map(|branch| PrunedBranch {
                        name: branch.name.clone(),
                        sha: branch.sha.clone(),
                    })
                    .collect();
                result.detail = format!("would delete {} branch(es)", branches.len());
            } else {
                let applied = prune::apply(runner, Path::new(&top), &branches);
                result.deleted = applied
                    .deleted
                    .iter()
                    .map(|branch| PrunedBranch {
                        name: branch.name.clone(),
                        sha: branch.sha.clone(),
                    })
                    .collect();
                result.failed = applied
                    .failed
                    .iter()
                    .map(|branch| branch.name.clone())
                    .collect();
                result.detail = applied.detail;
            }
        }
    }
    result
}

pub fn format_prune(onto: &str, dry: bool, json: bool, results: &[PruneRepoResult]) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let verb = if dry { "dry prune" } else { "prune" };
    let mut out = format!("{verb} (merged into '{onto}')\n\n");
    out.push_str(&format!("{:<30} {:<10} {}\n", "REPO", "BRANCHES", "DETAIL"));
    for result in results {
        let count = if result.failed.is_empty() {
            result.deleted.len().to_string()
        } else {
            format!("{}!{}", result.deleted.len(), result.failed.len())
        };
        out.push_str(&format!(
            "{:<30} {:<10} {}\n",
            result.name,
            count,
            result.detail.lines().next().unwrap_or("")
        ));
    }
    let deleted: usize = results.iter().map(|result| result.deleted.len()).sum();
    let failed: usize = results.iter().map(|result| result.failed.len()).sum();
    out.push_str(&format!(
        "\n{} repos: {} {}, {} failed",
        results.len(),
        deleted,
        if dry { "to delete" } else { "deleted" },
        failed
    ));
    out
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
        name: super::repo_name(&top),
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

    let dirty = dirty_state(&repo.path);
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

fn commit_one(repo: &ManagedRepo, options: &ManagedOptions) -> CommitResult {
    let state = dirty_state(&repo.path);
    let mut result = CommitResult {
        name: repo.name.clone(),
        present: state.present,
        dirty: state.dirty,
        files: state.files,
        action: String::new(),
        detail: String::new(),
    };

    if !state.present {
        result.action = "absent".to_string();
        result.detail = "not present on this machine".to_string();
        return result;
    }
    if !state.dirty {
        result.action = "clean".to_string();
        result.detail = "nothing to commit".to_string();
        return result;
    }
    if options.dry {
        result.action = "would-commit".to_string();
        result.detail = format!("{} change(s)", result.files.len());
        return result;
    }

    let Some(message) = options.message_for_all.as_ref() else {
        result.action = "skipped".to_string();
        result.detail = "blank message - skipped".to_string();
        return result;
    };

    match git_capture(&repo.path, &["add", "-A"]) {
        Ok(output) if output.success() => {}
        _ => {
            result.action = "fail".to_string();
            result.detail = "git add failed".to_string();
            return result;
        }
    }

    match git_capture(&repo.path, &["commit", "-m", message]) {
        Ok(output) if output.success() => {
            result.action = "committed".to_string();
            result.detail = last_non_empty_line(&output.combined())
                .unwrap_or("committed")
                .to_string();
        }
        Ok(output) => {
            result.action = "fail".to_string();
            result.detail = last_non_empty_line(&output.combined())
                .unwrap_or("commit failed")
                .to_string();
        }
        Err(error) => {
            result.action = "fail".to_string();
            result.detail = error.to_string();
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirtyState {
    present: bool,
    dirty: bool,
    files: Vec<CommitFile>,
}

fn dirty_state(repo: &Path) -> DirtyState {
    if !repo.join(".git").exists() {
        return DirtyState {
            present: false,
            dirty: false,
            files: Vec::new(),
        };
    }

    let output = match git_capture(repo, &["status", "--porcelain"]) {
        Ok(output) if output.success() => output.stdout,
        _ => String::new(),
    };
    let files = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| CommitFile {
            status: line.get(0..2).unwrap_or("").trim().to_string(),
            path: line.get(3..).unwrap_or("").to_string(),
        })
        .collect::<Vec<_>>();

    DirtyState {
        present: true,
        dirty: !files.is_empty(),
        files,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitCapture {
    stdout: String,
    stderr: String,
    code: i32,
}

impl GitCapture {
    fn success(&self) -> bool {
        self.code == 0
    }

    fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

fn git_capture(repo: &Path, args: &[&str]) -> anyhow::Result<GitCapture> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo.display()))?;

    Ok(GitCapture {
        stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
        stderr: String::from_utf8(output.stderr).context("git stderr was not valid UTF-8")?,
        code: output.status.code().unwrap_or(1),
    })
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

fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

pub fn format_push_pull(label: &str, dry: bool, json: bool, results: &[PushPullResult]) -> String {
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

pub fn format_commit(dry: bool, json: bool, results: &[CommitResult]) -> String {
    if json {
        return serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    }

    let mut out = String::new();
    out.push_str(&format!("{:<30} {:<14} {}\n", "REPO", "ACTION", "DETAIL"));
    for result in results {
        out.push_str(&format!(
            "{:<30} {:<14} {}\n",
            result.name, result.action, result.detail
        ));
    }
    let actions = results
        .iter()
        .map(|result| result.action.as_str())
        .collect::<Vec<_>>();
    out.push_str(&format!(
        "\nexit {}  -  {} repos",
        commit_exit_code(&actions, dry).code(),
        results.len()
    ));
    out
}

pub fn format_status(json: bool, color: bool, results: &[StatusResult]) -> String {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatusColorPalette {
    brackets: Rgb,
    ahead_arrow: Rgb,
    checkmark: Rgb,
    change_markers: Rgb,
}

impl StatusColorPalette {
    fn from_embedded_toml() -> Self {
        Self::from_toml(STATUS_COLORS_TOML).expect("embedded status color palette should be valid")
    }

    fn from_toml(raw: &str) -> anyhow::Result<Self> {
        let file: StatusColorPaletteToml =
            toml::from_str(raw).context("failed to parse embedded status color palette TOML")?;
        Ok(Self {
            brackets: Rgb::from_hex(&file.brackets_color)
                .context("invalid brackets_color in status color palette")?,
            ahead_arrow: Rgb::from_hex(&file.ahead_arrow_color)
                .context("invalid ahead_arrow_color in status color palette")?,
            checkmark: Rgb::from_hex(&file.checkmark_color)
                .context("invalid checkmark_color in status color palette")?,
            change_markers: Rgb::from_hex(&file.change_markers_color)
                .context("invalid change_markers_color in status color palette")?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct StatusColorPaletteToml {
    brackets_color: String,
    ahead_arrow_color: String,
    checkmark_color: String,
    change_markers_color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    fn from_hex(value: &str) -> anyhow::Result<Self> {
        let hex = value
            .strip_prefix('#')
            .ok_or_else(|| anyhow!("hex color must start with '#': {value}"))?;
        if hex.len() != 6 {
            anyhow::bail!("hex color must use #rrggbb form: {value}");
        }

        Ok(Self {
            red: parse_hex_channel(value, &hex[0..2])?,
            green: parse_hex_channel(value, &hex[2..4])?,
            blue: parse_hex_channel(value, &hex[4..6])?,
        })
    }

    fn paint(self, value: &str) -> String {
        format!(
            "\x1b[38;2;{};{};{}m{value}\x1b[39m",
            self.red, self.green, self.blue
        )
    }
}

fn parse_hex_channel(color: &str, channel: &str) -> anyhow::Result<u8> {
    u8::from_str_radix(channel, 16)
        .with_context(|| format!("hex color contains invalid digits: {color}"))
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn parses_toml_manifest_with_comments_and_missing_remote() {
        let repos = parse_manifest(
            "# comment\n\
             [[repo]]\npath = \"self/repo-b\"\nremote = \"https://example.invalid/cfg.git\"\ncode = \"CFG\"\n\n\
             [[repo]]\npath = \"tools/git-tools\"\n",
            Path::new("/home/me"),
        )
        .unwrap();

        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "self/repo-b");
        assert_eq!(repos[0].path, Path::new("/home/me").join("self/repo-b"));
        assert_eq!(repos[0].remote, "https://example.invalid/cfg.git");
        assert_eq!(repos[1].name, "tools/git-tools");
        assert_eq!(repos[1].remote, "");
    }

    #[test]
    fn resolve_repos_file_sources_respect_precedence() {
        let fixture = ManagedFixture::new("resolve-precedence");
        let explicit = fixture.root.join("explicit.txt");
        let env_file = fixture.root.join("env.txt");
        let cwd = fixture.root.join("workspace").join("repo");
        let upward = fixture
            .root
            .join("workspace/config/provisioning/linux/repos.toml");
        let home_default = fixture
            .home
            .join("self/sample_project/config/provisioning/linux/repos.toml");
        touch(&explicit);
        touch(&env_file);
        touch(&upward);
        touch(&home_default);

        assert_eq!(
            resolve_repos_file_from_sources(
                Some(&explicit),
                Some(&env_file),
                &cwd,
                Some(&fixture.home),
            )
            .unwrap(),
            explicit
        );
        assert_eq!(
            resolve_repos_file_from_sources(None, Some(&env_file), &cwd, Some(&fixture.home))
                .unwrap(),
            env_file
        );
        assert_eq!(
            resolve_repos_file_from_sources(None, None, &cwd, Some(&fixture.home)).unwrap(),
            upward
        );
    }

    #[test]
    fn resolve_repos_file_uses_home_default_after_upward_search_misses() {
        let fixture = ManagedFixture::new("resolve-home-default");
        let cwd = fixture.root.join("elsewhere").join("repo");
        let home_default = fixture
            .home
            .join("self/sample_project/config/provisioning/linux/repos.toml");
        touch(&home_default);

        assert_eq!(
            resolve_repos_file_from_sources(None, None, &cwd, Some(&fixture.home)).unwrap(),
            home_default
        );
    }

    #[test]
    fn resolve_repos_file_errors_without_any_config_source() {
        let fixture = ManagedFixture::new("resolve-missing");
        let cwd = fixture.root.join("elsewhere").join("repo");

        let err = resolve_repos_file_from_sources(None, None, &cwd, Some(&fixture.home))
            .expect_err("no manifest should be resolvable");
        let message = err.to_string();
        assert!(
            message.contains("--repos-file")
                && message.contains("GIT_TOOLS_MANAGED_REPOS_FILE")
                && message.contains("self/sample_project"),
            "error should point at supported manifest sources, got: {message}"
        );
    }

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
    fn commit_exit_codes_match_dry_and_real_modes() {
        assert_eq!(
            commit_exit_code(&["clean", "absent"], true),
            ManagedExit::Clean
        );
        assert_eq!(commit_exit_code(&["would-commit"], true), ManagedExit::Warn);
        assert_eq!(commit_exit_code(&["skipped"], false), ManagedExit::Warn);
        assert_eq!(commit_exit_code(&["fail"], false), ManagedExit::Fail);
    }

    #[test]
    fn status_colors_live_in_toml() {
        let raw = std::fs::read_to_string("config/status-colors.toml").unwrap();
        let file: StatusColorPaletteToml = toml::from_str(&raw).unwrap();

        assert_eq!(file.brackets_color, "#f28500");
        assert_eq!(file.ahead_arrow_color, "#f28500");
        assert_eq!(file.checkmark_color, "#2ecc71");
        assert_eq!(file.change_markers_color, "#ff4d4d");

        let source = std::fs::read_to_string("src/commands/managed.rs").unwrap();
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("managed.rs should contain production source");
        assert!(!production_source.contains("242;133;0m{value}"));
        assert!(!production_source.contains("\\x1b[31m{value}"));
        assert!(!production_source.contains("\\x1b[32m{value}"));
    }

    #[test]
    fn commit_all_dry_reports_dirty_without_committing() {
        let fixture = ManagedFixture::new("commit-dry");
        let repo = fixture.init_repo("repo");
        fixture.write_manifest(&[("repo", "")]);
        fixture.write_file("repo/work.txt", "dirty\n");
        let before = git_out(&repo, &["rev-parse", "HEAD"]);

        let run = run_commit_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: true,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Warn);
        assert_eq!(run.results[0].action, "would-commit");
        assert_eq!(git_out(&repo, &["rev-parse", "HEAD"]), before);
    }

    #[test]
    fn commit_all_message_for_all_commits_dirty_repo() {
        let fixture = ManagedFixture::new("commit-batch");
        let repo = fixture.init_repo("repo");
        fixture.write_manifest(&[("repo", "")]);
        fixture.write_file("repo/work.txt", "dirty\n");

        let run = run_commit_all(&ManagedOptions {
            repos_file: Some(fixture.manifest.clone()),
            home_dir: Some(fixture.home.clone()),
            dry: false,
            json: false,
            color: false,
            message_for_all: Some("save work".to_string()),
            interactive: false,
        });

        assert_eq!(run.exit, ManagedExit::Clean);
        assert_eq!(run.results[0].action, "committed");
        assert_eq!(git_out(&repo, &["status", "--porcelain"]), "");
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

    #[test]
    fn parse_manifest_skips_paused_repos() {
        let home = Path::new("/home/u");
        let raw = "[[repo]]\npath='self/cfg'\nremote='git@x:c.git'\n\
                   [[repo]]\npath='work/sample_project'\nremote='git@x:i.git'\nmanaged=false\n";
        let repos = parse_manifest(raw, home).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "self/cfg");
    }

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

    struct ManagedFixture {
        root: std::path::PathBuf,
        home: std::path::PathBuf,
        manifest: std::path::PathBuf,
    }

    impl ManagedFixture {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("git-tools-{name}-{unique}"));
            let home = root.join("home");
            std::fs::create_dir_all(&home).unwrap();
            let manifest = root.join("repos.toml");
            Self {
                root,
                home,
                manifest,
            }
        }

        fn init_repo(&self, name: &str) -> std::path::PathBuf {
            let repo = self.home.join(name);
            std::fs::create_dir_all(&repo).unwrap();
            cmd("git", &["init", "-b", "main", repo.to_str().unwrap()]);
            configure_repo(&repo);
            self.write_file(&format!("{name}/README.md"), "base\n");
            git(&repo, &["add", "-A"]);
            git(&repo, &["commit", "-m", "base"]);
            repo
        }

        fn origin_with_seed(&self) -> (std::path::PathBuf, std::path::PathBuf) {
            let origin = self.root.join("origin.git");
            cmd(
                "git",
                &[
                    "init",
                    "--bare",
                    "--initial-branch=main",
                    origin.to_str().unwrap(),
                ],
            );
            let seed = self.root.join("seed");
            cmd(
                "git",
                &["clone", origin.to_str().unwrap(), seed.to_str().unwrap()],
            );
            configure_repo(&seed);
            self.write_file_in(&seed, "README.md", "base\n");
            git(&seed, &["add", "-A"]);
            git(&seed, &["commit", "-m", "base"]);
            git(&seed, &["push", "-u", "origin", "main"]);
            (origin, seed)
        }

        fn clone_repo(&self, origin: &Path, name: &str) -> std::path::PathBuf {
            let repo = self.home.join(name);
            cmd(
                "git",
                &["clone", origin.to_str().unwrap(), repo.to_str().unwrap()],
            );
            configure_repo(&repo);
            repo
        }

        fn write_manifest(&self, entries: &[(&str, &str)]) {
            let text = entries
                .iter()
                .map(|(path, remote)| {
                    format!("[[repo]]\npath = \"{path}\"\nremote = \"{remote}\"\n\n")
                })
                .collect::<String>();
            std::fs::write(&self.manifest, text).unwrap();
        }

        fn write_file(&self, relative: &str, text: &str) {
            self.write_file_in(&self.home, relative, text);
        }

        fn write_file_in(&self, root: &Path, relative: &str, text: &str) {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, text).unwrap();
        }
    }

    impl Drop for ManagedFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn configure_repo(repo: &Path) {
        git(repo, &["config", "user.name", "Test User"]);
        git(repo, &["config", "user.email", "test@example.invalid"]);
        git(repo, &["config", "commit.gpgsign", "false"]);
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_out(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, "").unwrap();
    }

    fn cmd(program: &str, args: &[&str]) {
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{} {} failed\nstdout: {}\nstderr: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
