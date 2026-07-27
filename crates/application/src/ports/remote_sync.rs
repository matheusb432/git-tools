use std::{future::Future, path::Path};

/// One captured git subprocess result: exit success plus combined stdout+stderr.
/// Mirrors the CLI's retired `managed::git_capture::GitCapture::{success, combined}`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncOutput {
    pub success: bool,
    pub combined: String,
}

/// Async git remote operations behind `push --all`/`pull --all`. Every method mirrors exactly
/// one git invocation the CLI's retired `push_pull.rs` shelled out to; fallback rules
/// (empty branch on failure, treat a rev-list error as zero, etc.) stay in the
/// `managed` slices, not here - same split as [`crate::ports::DiffSource`].
pub trait RemoteSync: Clone + Send + Sync + 'static {
    /// Whether `repo`'s `.git` directory exists on this machine (pure fs check).
    fn repo_present(&self, repo: &Path) -> bool;

    /// `git rev-parse --abbrev-ref HEAD`.
    fn current_branch(&self, repo: &Path) -> impl Future<Output = anyhow::Result<String>> + Send;

    /// `git remote get-url <remote>`; `Ok(true)` iff it resolves.
    fn has_remote(
        &self,
        repo: &Path,
        remote: &str,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// `git push <remote> <branch> [--dry-run]`.
    fn push(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        dry: bool,
    ) -> impl Future<Output = anyhow::Result<SyncOutput>> + Send;

    /// `git fetch <remote>`.
    fn fetch(
        &self,
        repo: &Path,
        remote: &str,
    ) -> impl Future<Output = anyhow::Result<SyncOutput>> + Send;

    /// `git rev-parse --abbrev-ref --symbolic-full-name @{u}`; `Ok(None)` if no upstream.
    fn upstream_ref(
        &self,
        repo: &Path,
    ) -> impl Future<Output = anyhow::Result<Option<String>>> + Send;

    /// `git rev-parse --verify --quiet <remote_ref>`; `Ok(true)` iff it resolves.
    fn verify_ref(
        &self,
        repo: &Path,
        remote_ref: &str,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// `git rev-list --count <range>`.
    fn rev_list_count(
        &self,
        repo: &Path,
        range: &str,
    ) -> impl Future<Output = anyhow::Result<usize>> + Send;

    /// `git rev-list --count --left-right <range>` -> `(behind, ahead)`.
    fn rev_list_left_right(
        &self,
        repo: &Path,
        range: &str,
    ) -> impl Future<Output = anyhow::Result<(usize, usize)>> + Send;

    /// `git merge --ff-only <remote_ref>`.
    fn merge_ff_only(
        &self,
        repo: &Path,
        remote_ref: &str,
    ) -> impl Future<Output = anyhow::Result<SyncOutput>> + Send;
}
