//! The git-subprocess [`DiffSource`] adapter: pure delegation to `git_capture`.

use std::{collections::HashMap, path::Path};

use application::ports::DiffSource;
use domain::diffs::Commit;

/// Real git access for the diff engine. Every method shells out via `git_capture`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitDiffSource;

impl DiffSource for GitDiffSource {
    fn top_level(&self, dir: &Path) -> anyhow::Result<String> {
        crate::git_capture::top_level(dir)
    }
    fn current_branch(&self, repo: &Path) -> anyhow::Result<String> {
        crate::git_capture::current_branch(repo)
    }
    fn upstream(&self, repo: &Path) -> anyhow::Result<String> {
        crate::git_capture::upstream(repo)
    }
    fn verify_commit(&self, repo: &Path, rev: &str) -> anyhow::Result<()> {
        crate::git_capture::verify_commit(repo, rev)
    }
    fn short_ref(&self, repo: &Path, rev: &str) -> anyhow::Result<String> {
        crate::git_capture::short_ref(repo, rev)
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
    fn diff_raw(&self, repo: &Path, args: &[String]) -> anyhow::Result<String> {
        crate::git_capture::diff_raw(repo, args)
    }
    fn blame_forward(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<String> {
        crate::git_capture::blame_forward(repo, base, tip, path)
    }
    fn blame_forward_worktree(&self, repo: &Path, path: &str) -> anyhow::Result<String> {
        crate::git_capture::blame_forward_worktree(repo, path)
    }
    fn blame_reverse(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<String> {
        crate::git_capture::blame_reverse(repo, base, tip, path)
    }
    fn root_commit(&self, repo: &Path) -> Option<String> {
        crate::git_capture::root_commit(repo)
    }
    fn resolve_sha(&self, repo: &Path, rev: &str) -> anyhow::Result<String> {
        crate::git_capture::resolve_sha(repo, rev)
    }
    fn committed_at(&self, repo: &Path, rev: &str) -> String {
        crate::git_capture::committed_at(repo, rev)
    }
}
