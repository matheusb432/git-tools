//! The production Git adapter.

mod parsing;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::Context as _;
use gix::bstr::ByteSlice;
use gtl_application::ports::{
    CommitLogEntry, GitClient, GitCommitReceipt, GitDiffRequest, GitEffect, GitPushReceipt,
    GitRepositoryState, GitWorkingTree, MergedBranch,
};
use gtl_models::{
    diffs::{Commit, CommitId},
    git::{
        AheadBehind, BranchName, CommitCount, GitEffectMode, GitHead, GitObjectId, GitRange,
        GitRefName, GitRevision, RemoteName, RemoteUrl, TagName,
    },
    paths::RepositoryRoot,
    tags::Tag,
    timestamps::MachineTimestamp,
    worktrees::Worktree,
};

use self::parsing::{parse_local_tags, parse_remote_tags, parse_working_tree, parse_worktrees};

/// Production Git adapter using stable `gix` facade APIs with a private process fallback.
#[derive(Debug, Clone, Copy, Default)]
pub struct HybridGitClient;

impl GitClient for HybridGitClient {
    fn repo_present(&self, repo_path: &RepositoryRoot) -> bool {
        repo_path.as_ref().join(".git").exists()
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
        let top_level = RepositoryRoot::try_new(
            std::fs::canonicalize(top_level).unwrap_or_else(|_| top_level.into()),
        )?;
        Ok(GitRepositoryState::Repository { top_level })
    }

    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<RepositoryRoot>> {
        let Ok(repository) = gix::discover(dir) else {
            return Ok(None);
        };
        let Some(top_level) = repository.work_dir() else {
            return Ok(None);
        };
        Ok(Some(RepositoryRoot::try_new(
            std::fs::canonicalize(top_level)
                .with_context(|| format!("canonicalize Git worktree {}", top_level.display()))?,
        )?))
    }

    fn top_level(&self, dir: &Path) -> anyhow::Result<RepositoryRoot> {
        self.discover_top(dir)?
            .ok_or_else(|| anyhow::anyhow!("not a worktree repository: {}", dir.display()))
    }
    fn current_branch(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitHead> {
        let repository = gix::discover(repo_path)?;
        repository
            .head_name()?
            .map_or(Ok(GitHead::Detached), |name| {
                BranchName::try_new(name.shorten().to_str_lossy().into_owned())
                    .map(GitHead::Branch)
                    .map_err(Into::into)
            })
    }
    fn upstream(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<GitRefName>> {
        effect_result(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            |output| GitRefName::try_new(output.trim().to_owned()).map_err(Into::into),
        )
    }
    fn branch_remote(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
    ) -> anyhow::Result<Option<RemoteName>> {
        capture(repo_path, &["config", &format!("branch.{branch}.remote")])?
            .map(RemoteName::try_new)
            .transpose()
            .map_err(Into::into)
    }
    fn remote_url(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<Option<RemoteUrl>> {
        capture(repo_path, &["remote", "get-url", remote.as_ref()])?
            .map(RemoteUrl::try_new)
            .transpose()
            .map_err(Into::into)
    }
    fn revision_exists(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<bool> {
        succeeds(repo_path, &["rev-parse", "--verify", revision.as_ref()])
    }
    fn commit_count(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Option<CommitCount>> {
        Ok(
            capture(repo_path, &["rev-list", "--count", range.as_ref()])?
                .and_then(|count| count.parse().ok())
                .map(CommitCount::new),
        )
    }
    fn ahead_behind(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Option<AheadBehind>> {
        Ok(capture(
            repo_path,
            &["rev-list", "--count", "--left-right", range.as_ref()],
        )?
        .and_then(|counts| {
            let (left, right) = counts.split_once(char::is_whitespace)?;
            Some(AheadBehind {
                behind: CommitCount::new(left.parse().ok()?),
                ahead: CommitCount::new(right.trim().parse().ok()?),
            })
        }))
    }
    fn is_ancestor(
        &self,
        repo_path: &RepositoryRoot,
        ancestor: &GitRevision,
        descendant: &GitRevision,
    ) -> anyhow::Result<bool> {
        succeeds(
            repo_path,
            &[
                "merge-base",
                "--is-ancestor",
                ancestor.as_ref(),
                descendant.as_ref(),
            ],
        )
    }
    fn working_tree(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        effect_result(repo_path, &["status", "--porcelain"], parse_working_tree)
    }
    fn merged_branches(
        &self,
        repo_path: &RepositoryRoot,
        into: &GitRevision,
    ) -> anyhow::Result<GitEffect<Vec<MergedBranch>>> {
        match effect(
            repo_path,
            &[
                "for-each-ref",
                "--merged",
                into.as_ref(),
                "--format=%(refname:short) %(objectname)",
                "refs/heads/",
            ],
            |output| -> anyhow::Result<Vec<MergedBranch>> {
                output
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|line| {
                        let (name, raw_id) = line
                            .split_once(char::is_whitespace)
                            .ok_or_else(|| anyhow::anyhow!("Git branch output omitted its ID"))?;
                        Ok(MergedBranch {
                            name: BranchName::try_new(name.to_owned())?,
                            id: raw_id.trim().try_into()?,
                        })
                    })
                    .collect()
            },
        )? {
            GitEffect::Applied(branches) => branches.map(GitEffect::Applied),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn worktrees(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<Vec<Worktree>>> {
        match effect(
            repo_path,
            &["worktree", "list", "--porcelain"],
            parse_worktrees,
        )? {
            GitEffect::Applied(worktrees) => worktrees.map(GitEffect::Applied),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn local_tags(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, Tag>>> {
        match effect(
            repo_path,
            &[
                "for-each-ref",
                "--format=%(objectname)\t%(*objectname)\t%(refname:strip=2)\t%(contents:lines=1)\t%(creatordate:unix)",
                "refs/tags",
            ],
            parse_local_tags,
        )? {
            GitEffect::Applied(tags) => tags.map(GitEffect::Applied),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn remote_tags(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, GitObjectId>>> {
        effect_result(
            repo_path,
            &["ls-remote", "--tags", remote.as_ref()],
            parse_remote_tags,
        )
    }
    fn previous_checkout(&self, repo_path: &RepositoryRoot) -> anyhow::Result<Option<GitRevision>> {
        capture(repo_path, &["rev-parse", "@{-1}"])?
            .map(GitRevision::try_new)
            .transpose()
            .map_err(Into::into)
    }
    fn brief_log(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<GitEffect<Vec<CommitLogEntry>>> {
        match effect(
            repo_path,
            &["log", "--format=%H%x1f%s", range.as_ref()],
            parse_brief_log,
        )? {
            GitEffect::Applied(commits) => commits.map(GitEffect::Applied),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn diff_stat(
        &self,
        repo_path: &RepositoryRoot,
        before: &GitRevision,
        after: &GitRevision,
    ) -> anyhow::Result<GitEffect<String>> {
        effect(
            repo_path,
            &["diff", "--stat", before.as_ref(), after.as_ref()],
            str::to_string,
        )
    }
    fn stage_all(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<()>> {
        effect(repo_path, &["add", "-A"], |_| ())
    }
    fn commit(
        &self,
        repo_path: &RepositoryRoot,
        message: &str,
    ) -> anyhow::Result<GitEffect<GitCommitReceipt>> {
        let result = effect(repo_path, &["commit", "-m", message], |stdout| {
            last_line(stdout).unwrap_or("committed").to_string()
        })?;
        Ok(match result {
            GitEffect::Applied(detail) => GitEffect::Applied(GitCommitReceipt {
                id: self.resolve_commit_id(repo_path, &GitRevision::head())?,
                detail,
            }),
            GitEffect::Rejected(detail) => GitEffect::Rejected(detail),
        })
    }
    fn switch(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(repo_path, &["switch", branch.as_ref()], |_| ())
    }
    fn switch_previous(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<()>> {
        effect(repo_path, &["switch", "-"], |_| ())
    }
    fn fast_forward(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<String>> {
        effect(
            repo_path,
            &["merge", "--ff-only", revision.as_ref()],
            str::to_string,
        )
    }
    fn move_branch(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(
            repo_path,
            &["branch", "-f", branch.as_ref(), revision.as_ref()],
            |_| (),
        )
    }
    fn delete_branch(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(repo_path, &["branch", "-D", branch.as_ref()], |_| ())
    }
    fn soft_reset(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(repo_path, &["reset", "--soft", revision.as_ref()], |_| ())
    }
    fn push_branch(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
        branch: &BranchName,
        mode: GitEffectMode,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>> {
        let mut args = vec!["push", remote.as_ref(), branch.as_ref()];
        if mode.is_dry_run() {
            args.push("--dry-run");
        }
        let output = raw(repo_path, &args)?;
        if !output.success() {
            return Ok(GitEffect::Rejected(output.diagnostic().to_string()));
        }
        let combined = output.combined();
        let detail = last_line(&combined).unwrap_or("pushed").to_string();
        Ok(GitEffect::Applied(
            if combined.contains("Everything up-to-date") {
                GitPushReceipt::UpToDate { detail }
            } else {
                GitPushReceipt::Updated { detail }
            },
        ))
    }
    fn fetch(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<String>> {
        effect(repo_path, &["fetch", remote.as_ref()], str::to_string)
    }
    fn create_annotated_tag(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(
            repo_path,
            &["tag", "-a", tag.as_ref(), "-m", message],
            |_| (),
        )
    }
    fn create_annotated_tag_at(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        revision: &GitRevision,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(
            repo_path,
            &["tag", "-a", tag.as_ref(), revision.as_ref(), "-m", message],
            |_| (),
        )
    }
    fn create_lightweight_tag(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<()>> {
        effect(
            repo_path,
            &["tag", tag.as_ref(), &format!("{revision}^{{}}")],
            |_| (),
        )
    }
    fn push_tag_refs(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
        tags: &BTreeSet<GitRefName>,
    ) -> anyhow::Result<GitEffect<String>> {
        let mut owned = vec!["push".to_string(), remote.to_string()];
        owned.extend(
            tags.iter()
                .map(|reference| format!("{reference}:{reference}")),
        );
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        effect(repo_path, &args, str::to_string)
    }
    fn verify_commit(&self, repo_path: &RepositoryRoot, rev: &GitRevision) -> anyhow::Result<()> {
        let repository = gix::discover(repo_path)?;
        repository.rev_parse_single(format!("{rev}^{{commit}}").as_bytes().as_bstr())?;
        Ok(())
    }
    fn log_commits(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Vec<Commit>> {
        crate::git_capture::log_commits(repo_path, range)
    }
    fn diff(&self, repo_path: &RepositoryRoot, request: &GitDiffRequest) -> anyhow::Result<String> {
        crate::git_capture::diff(repo_path, request)
    }
    fn root_commit(&self, repo_path: &RepositoryRoot) -> Option<CommitId> {
        crate::git_capture::root_commit(repo_path)
    }
    fn resolve_commit_id(
        &self,
        repo_path: &RepositoryRoot,
        rev: &GitRevision,
    ) -> anyhow::Result<CommitId> {
        let repository = gix::discover(repo_path)?;
        let commit_revision = format!("{rev}^{{commit}}");
        let raw_id = match repository.rev_parse_single(commit_revision.as_bytes().as_bstr()) {
            Ok(object) => object.detach().to_string(),
            Err(native_error) => capture(repo_path, &["rev-parse", &commit_revision])?
                .ok_or_else(|| anyhow::Error::new(native_error))?,
        };
        raw_id.try_into().map_err(Into::into)
    }
    fn merge_base(
        &self,
        repo_path: &RepositoryRoot,
        left: &GitRevision,
        right: &GitRevision,
    ) -> anyhow::Result<CommitId> {
        crate::git_capture::merge_base(repo_path, left, right)
    }
    fn committed_at(
        &self,
        repo_path: &RepositoryRoot,
        rev: &GitRevision,
    ) -> Option<MachineTimestamp> {
        crate::git_capture::committed_at(repo_path, rev)
    }
}

fn raw(repo_path: &Path, args: &[&str]) -> anyhow::Result<crate::git_process::GitProcessOutput> {
    crate::git_process::run(repo_path, args)
}

fn capture(repo_path: &Path, args: &[&str]) -> anyhow::Result<Option<String>> {
    let output = raw(repo_path, args)?;
    Ok(output
        .success()
        .then(|| output.stdout.trim().to_string())
        .filter(|output| !output.is_empty()))
}

fn succeeds(repo_path: &Path, args: &[&str]) -> anyhow::Result<bool> {
    raw(repo_path, args).map(|output| output.success())
}

fn effect<T>(
    repo_path: &Path,
    args: &[&str],
    applied: impl FnOnce(&str) -> T,
) -> anyhow::Result<GitEffect<T>> {
    let output = raw(repo_path, args)?;
    if output.success() {
        Ok(GitEffect::Applied(applied(&output.combined())))
    } else {
        Ok(GitEffect::Rejected(output.error_line()))
    }
}

fn effect_result<T>(
    repo_path: &Path,
    args: &[&str],
    applied: impl FnOnce(&str) -> anyhow::Result<T>,
) -> anyhow::Result<GitEffect<T>> {
    match effect(repo_path, args, applied)? {
        GitEffect::Applied(value) => value.map(GitEffect::Applied),
        GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
    }
}

fn last_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn parse_brief_log(output: &str) -> anyhow::Result<Vec<CommitLogEntry>> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (raw_id, subject) = line
                .split_once('\x1f')
                .ok_or_else(|| anyhow::anyhow!("git log entry omitted its subject delimiter"))?;
            let id: CommitId = raw_id.try_into()?;
            Ok(CommitLogEntry {
                id,
                subject: subject.trim().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_brief_log;

    #[test]
    fn brief_log_retains_validated_full_commit_ids() {
        let output = concat!(
            "1111111111111111111111111111111111111111\x1ffeat: one\n",
            "2222222222222222222222222222222222222222\x1ffix: two\n"
        );

        let commits = parse_brief_log(output).expect("valid brief log");
        assert_eq!(
            commits[0].id.to_string(),
            "1111111111111111111111111111111111111111"
        );
        assert_eq!(commits[0].subject, "feat: one");
        assert_eq!(
            commits[1].id.to_string(),
            "2222222222222222222222222222222222222222"
        );
        assert_eq!(commits[1].subject, "fix: two");
        assert!(parse_brief_log("invalid\x1fsubject").is_err());
    }
}
