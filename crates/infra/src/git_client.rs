//! The production Git adapter.

mod parsing;

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use application::ports::{
    BlameLines, GitClient, GitCommitReceipt, GitDiffRequest, GitEffect, GitPushReceipt,
    GitRepositoryState, GitWorkingTree, MergedBranch,
};
use domain::{diffs::Commit, tags::Tag, worktrees::Worktree};
use gix::bstr::ByteSlice;

use self::parsing::{parse_local_tags, parse_remote_tags, parse_working_tree, parse_worktrees};

/// Production Git adapter using stable `gix` facade APIs with a private process fallback.
#[derive(Debug, Clone, Copy, Default)]
pub struct HybridGitClient;

impl GitClient for HybridGitClient {
    fn repo_present(&self, repo: &Path) -> bool {
        repo.join(".git").exists()
    }

    fn probe_repository(&self, dir: &Path) -> anyhow::Result<GitRepositoryState> {
        if !dir.is_dir() {
            return Ok(GitRepositoryState::NotFound);
        }
        let Ok(repository) = gix::discover(dir) else {
            return Ok(GitRepositoryState::NotARepository);
        };
        let Some(top_level) = repository.work_dir() else {
            return Ok(GitRepositoryState::NotARepository);
        };
        let top_level = std::fs::canonicalize(top_level).unwrap_or_else(|_| top_level.into());
        Ok(GitRepositoryState::Repository { top_level })
    }

    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<PathBuf>> {
        Ok(gix::discover(dir)
            .ok()
            .and_then(|repository| repository.work_dir().map(Path::to_path_buf)))
    }

    fn top_level(&self, dir: &Path) -> anyhow::Result<String> {
        let repository = gix::discover(dir)?;
        repository
            .work_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .ok_or_else(|| anyhow::anyhow!("not a worktree repository: {}", dir.display()))
    }
    fn current_branch(&self, repo: &Path) -> anyhow::Result<String> {
        let repository = gix::discover(repo)?;
        Ok(repository.head_name()?.map_or_else(
            || "HEAD".into(),
            |name| name.shorten().to_str_lossy().into_owned(),
        ))
    }
    fn upstream(&self, repo: &Path) -> anyhow::Result<GitEffect<String>> {
        effect(
            repo,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            |output| output.trim().to_string(),
        )
    }
    fn branch_remote(&self, repo: &Path, branch: &str) -> anyhow::Result<Option<String>> {
        capture(repo, &["config", &format!("branch.{branch}.remote")])
    }
    fn remote_url(&self, repo: &Path, remote: &str) -> anyhow::Result<Option<String>> {
        capture(repo, &["remote", "get-url", remote])
    }
    fn revision_exists(&self, repo: &Path, revision: &str) -> anyhow::Result<bool> {
        succeeds(repo, &["rev-parse", "--verify", revision])
    }
    fn commit_count(&self, repo: &Path, range: &str) -> anyhow::Result<Option<usize>> {
        Ok(capture(repo, &["rev-list", "--count", range])?.and_then(|count| count.parse().ok()))
    }
    fn ahead_behind(&self, repo: &Path, range: &str) -> anyhow::Result<Option<(usize, usize)>> {
        Ok(
            capture(repo, &["rev-list", "--count", "--left-right", range])?.and_then(|counts| {
                let (left, right) = counts.split_once(char::is_whitespace)?;
                Some((left.parse().ok()?, right.trim().parse().ok()?))
            }),
        )
    }
    fn is_ancestor(&self, repo: &Path, ancestor: &str, descendant: &str) -> anyhow::Result<bool> {
        succeeds(repo, &["merge-base", "--is-ancestor", ancestor, descendant])
    }
    fn working_tree(&self, repo: &Path) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        effect(repo, &["status", "--porcelain"], parse_working_tree)
    }
    fn merged_branches(
        &self,
        repo: &Path,
        into: &str,
    ) -> anyhow::Result<GitEffect<Vec<MergedBranch>>> {
        effect(
            repo,
            &[
                "for-each-ref",
                "--merged",
                into,
                "--format=%(refname:short) %(objectname:short)",
                "refs/heads/",
            ],
            |output| {
                output
                    .lines()
                    .filter_map(|line| {
                        let (name, sha) = line.trim().split_once(char::is_whitespace)?;
                        Some(MergedBranch {
                            name: name.to_string(),
                            sha: sha.trim().to_string(),
                        })
                    })
                    .collect()
            },
        )
    }
    fn worktrees(&self, repo: &Path) -> anyhow::Result<GitEffect<Vec<Worktree>>> {
        effect(repo, &["worktree", "list", "--porcelain"], parse_worktrees)
    }
    fn local_tags(&self, repo: &Path) -> anyhow::Result<GitEffect<BTreeMap<String, Tag>>> {
        effect(
            repo,
            &[
                "for-each-ref",
                "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=2)\t%(contents:lines=1)\t%(creatordate:unix)",
                "refs/tags",
            ],
            parse_local_tags,
        )
    }
    fn remote_tags(
        &self,
        repo: &Path,
        remote: &str,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, String>>> {
        effect(repo, &["ls-remote", "--tags", remote], parse_remote_tags)
    }
    fn previous_checkout(&self, repo: &Path) -> anyhow::Result<Option<String>> {
        capture(repo, &["rev-parse", "@{-1}"])
    }
    fn brief_log(&self, repo: &Path, range: &str) -> anyhow::Result<GitEffect<Vec<String>>> {
        effect(repo, &["log", "--format=%h %s", range], |output| {
            output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
    }
    fn diff_stat(
        &self,
        repo: &Path,
        before: &str,
        after: &str,
    ) -> anyhow::Result<GitEffect<String>> {
        effect(repo, &["diff", "--stat", before, after], str::to_string)
    }
    fn stage_all(&self, repo: &Path) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["add", "-A"], |_| ())
    }
    fn commit(&self, repo: &Path, message: &str) -> anyhow::Result<GitEffect<GitCommitReceipt>> {
        effect(repo, &["commit", "-m", message], |stdout| {
            GitCommitReceipt {
                identity: commit_identity(stdout),
                detail: last_line(stdout).unwrap_or("committed").to_string(),
            }
        })
    }
    fn switch(&self, repo: &Path, branch: &str) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["switch", branch], |_| ())
    }
    fn switch_previous(&self, repo: &Path) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["switch", "-"], |_| ())
    }
    fn fast_forward(&self, repo: &Path, revision: &str) -> anyhow::Result<GitEffect<String>> {
        effect(repo, &["merge", "--ff-only", revision], str::to_string)
    }
    fn move_branch(
        &self,
        repo: &Path,
        branch: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["branch", "-f", branch, revision], |_| ())
    }
    fn delete_branch(&self, repo: &Path, branch: &str) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["branch", "-D", branch], |_| ())
    }
    fn soft_reset(&self, repo: &Path, revision: &str) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["reset", "--soft", revision], |_| ())
    }
    fn push_branch(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        dry_run: bool,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>> {
        let mut args = vec!["push", remote, branch];
        if dry_run {
            args.push("--dry-run");
        }
        let output = raw(repo, &args)?;
        if !output.success() {
            return Ok(GitEffect::Rejected(output.diagnostic().to_string()));
        }
        let combined = output.combined();
        Ok(GitEffect::Applied(GitPushReceipt {
            up_to_date: combined.contains("Everything up-to-date"),
            detail: last_line(&combined).unwrap_or("pushed").to_string(),
        }))
    }
    fn fetch(&self, repo: &Path, remote: &str) -> anyhow::Result<GitEffect<String>> {
        effect(repo, &["fetch", remote], str::to_string)
    }
    fn create_annotated_tag(
        &self,
        repo: &Path,
        tag: &str,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["tag", "-a", tag, "-m", message], |_| ())
    }
    fn create_lightweight_tag(
        &self,
        repo: &Path,
        tag: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(repo, &["tag", tag, &format!("{revision}^{{}}")], |_| ())
    }
    fn push_tag_refs(
        &self,
        repo: &Path,
        remote: &str,
        tags: &[String],
    ) -> anyhow::Result<GitEffect<String>> {
        let mut owned = vec!["push".to_string(), remote.to_string()];
        owned.extend(
            tags.iter()
                .map(|tag| format!("refs/tags/{tag}:refs/tags/{tag}")),
        );
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        effect(repo, &args, str::to_string)
    }
    fn verify_commit(&self, repo: &Path, rev: &str) -> anyhow::Result<()> {
        let repository = gix::discover(repo)?;
        repository.rev_parse_single(format!("{rev}^{{commit}}").as_bytes().as_bstr())?;
        Ok(())
    }
    fn short_ref(&self, repo: &Path, rev: &str) -> anyhow::Result<String> {
        Ok(self.resolve_sha(repo, rev)?.chars().take(7).collect())
    }
    fn log_commits(&self, repo: &Path, range: &str) -> anyhow::Result<Vec<Commit>> {
        crate::git_capture::log_commits(repo, range)
    }
    fn file_commit_map(
        &self,
        repo: &Path,
        range: &str,
    ) -> anyhow::Result<HashMap<String, Vec<String>>> {
        crate::git_capture::file_commit_map(repo, range)
    }
    fn merge_members(&self, repo: &Path, merge: &str, base: &str) -> anyhow::Result<Vec<String>> {
        crate::git_capture::merge_members(repo, merge, base)
    }
    fn diff(&self, repo: &Path, request: &GitDiffRequest) -> anyhow::Result<String> {
        crate::git_capture::diff(repo, request)
    }
    fn blame_forward(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<BlameLines> {
        crate::git_capture::blame_forward(repo, base, tip, path)
    }
    fn blame_forward_worktree(&self, repo: &Path, path: &str) -> anyhow::Result<BlameLines> {
        crate::git_capture::blame_forward_worktree(repo, path)
    }
    fn blame_reverse(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<BlameLines> {
        crate::git_capture::blame_reverse(repo, base, tip, path)
    }
    fn root_commit(&self, repo: &Path) -> Option<String> {
        crate::git_capture::root_commit(repo)
    }
    fn resolve_sha(&self, repo: &Path, rev: &str) -> anyhow::Result<String> {
        let repository = gix::discover(repo)?;
        match repository.rev_parse_single(rev.as_bytes().as_bstr()) {
            Ok(object) => Ok(object.detach().to_string()),
            Err(native_error) => {
                capture(repo, &["rev-parse", rev])?.ok_or_else(|| anyhow::Error::new(native_error))
            }
        }
    }
    fn merge_base(&self, repo: &Path, left: &str, right: &str) -> anyhow::Result<String> {
        crate::git_capture::merge_base(repo, left, right)
    }
    fn committed_at(&self, repo: &Path, rev: &str) -> String {
        crate::git_capture::committed_at(repo, rev)
    }
}

fn raw(repo: &Path, args: &[&str]) -> anyhow::Result<crate::git_process::GitProcessOutput> {
    crate::git_process::run(repo, args)
}

fn capture(repo: &Path, args: &[&str]) -> anyhow::Result<Option<String>> {
    let output = raw(repo, args)?;
    Ok(output
        .success()
        .then(|| output.stdout.trim().to_string())
        .filter(|output| !output.is_empty()))
}

fn succeeds(repo: &Path, args: &[&str]) -> anyhow::Result<bool> {
    raw(repo, args).map(|output| output.success())
}

fn effect<T>(
    repo: &Path,
    args: &[&str],
    applied: impl FnOnce(&str) -> T,
) -> anyhow::Result<GitEffect<T>> {
    let output = raw(repo, args)?;
    if output.success() {
        Ok(GitEffect::Applied(applied(&output.combined())))
    } else {
        Ok(GitEffect::Rejected(output.error_line()))
    }
}

fn last_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn commit_identity(output: &str) -> Option<String> {
    output.lines().rev().find_map(|line| {
        let (header, subject) = line.trim().strip_prefix('[')?.split_once(']')?;
        if subject.trim().is_empty() {
            return None;
        }
        let identity = header.split_whitespace().next_back()?;
        ((4..=64).contains(&identity.len())
            && identity.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| identity.to_string())
    })
}
