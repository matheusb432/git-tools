//! The production Git adapter.

mod parsing;
mod working_tree;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::Context as _;
use gix::bstr::ByteSlice;
use gtl_application::ports::{
    GitClient, GitCommitReceipt, GitDiffRequest, GitEffect, GitPushReceipt, GitRepositoryState,
    GitStatusSnapshot, GitStatusUpstream, GitWorkingTree, GitWorkingTreeSummary,
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
};

use self::parsing::{parse_local_tags, parse_remote_tags};

/// Production Git adapter using stable `gix` facade APIs with a private process fallback.
#[derive(Debug, Clone, Copy, Default)]
pub struct HybridGitClient;

impl GitClient for HybridGitClient {
    fn primary_worktree(&self, path: &RepositoryRoot) -> anyhow::Result<RepositoryRoot> {
        let repository = gix::discover(path.as_ref())?;
        if repository.git_dir() == repository.common_dir() {
            return Ok(path.clone());
        }
        let primary = gix::open(repository.common_dir())?;
        primary.workdir().map_or_else(
            || Ok(path.clone()),
            |directory| {
                RepositoryRoot::try_new(std::fs::canonicalize(directory)?).map_err(Into::into)
            },
        )
    }

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
        let Some(top_level) = repository.workdir() else {
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
        let Some(top_level) = repository.workdir() else {
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
        repository_head(&repository)
    }
    fn head_state(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<gtl_models::git::GitHeadState> {
        use gtl_models::git::GitHeadState;
        let repository = gix::open(repo_path.as_ref())?;
        let head = repository.head()?;
        let branch = head
            .referent_name()
            .map(|name| BranchName::try_new(name.shorten().to_str_lossy().into_owned()))
            .transpose()?;
        match head.id() {
            Some(id) => Ok(GitHeadState::Commit {
                head: branch.map_or(GitHead::Detached, GitHead::Branch),
                id: CommitId::try_new(id.to_string())?,
            }),
            None => Ok(GitHeadState::Unborn {
                branch: branch.ok_or_else(|| anyhow::anyhow!("unborn HEAD has no branch"))?,
            }),
        }
    }
    fn upstream(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<GitRefName>> {
        let repository = gix::open(repo_path.as_ref())?;
        let Some(upstream) = repository_upstream(&repository, repo_path)? else {
            return Ok(GitEffect::Rejected("current branch has no upstream".into()));
        };
        Ok(GitEffect::Applied(upstream))
    }
    fn branch_remote(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
    ) -> anyhow::Result<Option<RemoteName>> {
        let repository = gix::open(repo_path.as_ref())?;
        let branch_name: &str = branch.as_ref();
        repository
            .branch_remote_name(
                branch_name.as_bytes().as_bstr(),
                gix::remote::Direction::Fetch,
            )
            .map(|remote| RemoteName::try_new(remote.as_bstr().to_str_lossy().into_owned()))
            .transpose()
            .map_err(Into::into)
    }
    fn remote_url(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<Option<RemoteUrl>> {
        let repository = gix::open(repo_path.as_ref())?;
        let remote_name: &str = remote.as_ref();
        let Some(remote) = repository
            .try_find_remote(remote_name.as_bytes().as_bstr())
            .transpose()?
        else {
            return Ok(None);
        };
        remote
            .url(gix::remote::Direction::Fetch)
            .map(|url| RemoteUrl::try_new(url.to_bstring().to_str_lossy().into_owned()))
            .transpose()
            .map_err(Into::into)
    }
    fn remote_push_urls(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<Vec<RemoteUrl>> {
        let repository = gix::open(repo_path.as_ref())?;
        let remote_name: &str = remote.as_ref();
        let Some(remote) = repository
            .try_find_remote(remote_name.as_bytes().as_bstr())
            .transpose()?
        else {
            return Ok(Vec::new());
        };
        remote
            .urls(gix::remote::Direction::Push)
            .map(|url| RemoteUrl::try_new(url.to_bstring().to_str_lossy().into_owned()))
            .collect::<Result<Vec<_>, _>>()
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
    fn status_snapshot(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<GitStatusSnapshot>> {
        status_snapshot(repo_path, &BTreeMap::new())
    }
    fn status_snapshot_with_known_descendants(
        &self,
        repo_path: &RepositoryRoot,
        known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
    ) -> anyhow::Result<GitEffect<GitStatusSnapshot>> {
        status_snapshot(repo_path, known_descendants)
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
    fn working_tree(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        working_tree::read(repo_path).map(GitEffect::Applied)
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

    fn find_merge_base(
        &self,
        repo_path: &RepositoryRoot,
        left: &GitRevision,
        right: &GitRevision,
    ) -> anyhow::Result<Option<CommitId>> {
        let output = crate::git_process::run(
            repo_path.as_ref(),
            &["merge-base", left.as_ref(), right.as_ref()],
        )?;
        if output.exit_code == 1 {
            return Ok(None);
        }
        anyhow::ensure!(output.success(), "{}", output.error_line());
        output
            .stdout
            .trim()
            .try_into()
            .map(Some)
            .map_err(Into::into)
    }
    fn committed_at(
        &self,
        repo_path: &RepositoryRoot,
        rev: &GitRevision,
    ) -> Option<MachineTimestamp> {
        crate::git_capture::committed_at(repo_path, rev)
    }
}

fn status_snapshot(
    repo_path: &RepositoryRoot,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> anyhow::Result<GitEffect<GitStatusSnapshot>> {
    let repository = gix::open(repo_path.as_ref()).context("open Git repository")?;
    let head = repository_head(&repository)?;
    let upstream = repository_upstream(&repository, repo_path)?
        .map(|reference| -> anyhow::Result<GitStatusUpstream> {
            let ahead = capture(repo_path, &["rev-list", "--count", "@{u}..HEAD"])?
                .and_then(|count| count.parse().ok())
                .map(CommitCount::new)
                .unwrap_or_default();
            Ok(GitStatusUpstream { reference, ahead })
        })
        .transpose()?;
    let working_tree =
        working_tree::read_repository_with_known_descendants(&repository, known_descendants)?;
    Ok(GitEffect::Applied(GitStatusSnapshot {
        head,
        upstream,
        working_tree,
    }))
}

fn repository_head(repository: &gix::Repository) -> anyhow::Result<GitHead> {
    repository
        .head_name()?
        .map_or(Ok(GitHead::Detached), |name| {
            BranchName::try_new(name.shorten().to_str_lossy().into_owned())
                .map(GitHead::Branch)
                .map_err(Into::into)
        })
}

fn repository_upstream(
    repository: &gix::Repository,
    repo_path: &RepositoryRoot,
) -> anyhow::Result<Option<GitRefName>> {
    let Some(branch) = repository.head_name()? else {
        return Ok(None);
    };
    let Some(upstream) =
        repository.branch_remote_tracking_ref_name(branch.as_ref(), gix::remote::Direction::Fetch)
    else {
        return capture(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )?
        .map(GitRefName::try_new)
        .transpose()
        .map_err(Into::into);
    };
    Ok(Some(GitRefName::try_new(
        upstream?.shorten().to_str_lossy().into_owned(),
    )?))
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
