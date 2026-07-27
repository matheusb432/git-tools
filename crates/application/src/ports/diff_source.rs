use std::{collections::HashMap, path::Path};

use domain::diffs::Commit;

/// Read-only git access for the diff engine. Every method shells out to git in the
/// real adapter; the fake scripts each return value.
pub trait DiffSource: Clone + Send + Sync + 'static {
    /// Absolute path of the repository containing `dir`; errors when `dir` is not a git repo.
    fn top_level(&self, dir: &Path) -> anyhow::Result<String>;

    /// The current branch name (`HEAD`'s `--abbrev-ref`).
    fn current_branch(&self, repo: &Path) -> anyhow::Result<String>;

    /// The configured upstream tracking ref; errors when no upstream is configured.
    fn upstream(&self, repo: &Path) -> anyhow::Result<String>;

    /// Confirm `rev` resolves to a commit; errors when it does not.
    fn verify_commit(&self, repo: &Path, rev: &str) -> anyhow::Result<()>;

    /// The abbreviated sha for `rev`.
    fn short_ref(&self, repo: &Path, rev: &str) -> anyhow::Result<String>;

    /// The commits in `range`, newest first.
    fn log_commits(&self, repo: &Path, range: &str) -> anyhow::Result<Vec<Commit>>;

    /// Map each changed path in `range` to the short shas that touched it.
    fn file_commit_map(
        &self,
        repo: &Path,
        range: &str,
    ) -> anyhow::Result<HashMap<String, Vec<String>>>;

    /// The commits `merge` brought into `base..merge` (empty when it introduces nothing).
    fn merge_members(&self, repo: &Path, merge: &str, base: &str) -> anyhow::Result<Vec<String>>;

    /// Raw `git diff` output for `args` (the first arg is the `diff` subcommand).
    fn diff_raw(&self, repo: &Path, args: &[String]) -> anyhow::Result<String>;

    /// Range-bounded forward blame porcelain of `path` at `base..tip`.
    fn blame_forward(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<String>;

    /// Working-tree forward blame porcelain of `path` (hash mode: base -> worktree).
    fn blame_forward_worktree(&self, repo: &Path, path: &str) -> anyhow::Result<String>;

    /// Reverse blame porcelain of `path` at `base..tip` (each deleted line carries its deleter).
    fn blame_reverse(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<String>;

    /// The repo's stable oldest root-commit sha, or `None` for a repo with no commits.
    fn root_commit(&self, repo: &Path) -> Option<String>;

    /// Resolve `rev` to its full 40-char sha.
    fn resolve_sha(&self, repo: &Path, rev: &str) -> anyhow::Result<String>;

    /// The committer date of `rev` as a display string; never fails (empty on error).
    fn committed_at(&self, repo: &Path, rev: &str) -> String;
}
