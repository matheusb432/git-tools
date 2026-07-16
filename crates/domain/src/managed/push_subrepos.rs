//! Data shapes for the recursive `push -r` flow: destinations, plan, and applied
//! outcomes. Dumb carriers — the `push_subrepos` application slice builds and
//! interprets them; no behavior lives here.

use std::path::PathBuf;

/// Where a discovered repo's current branch would be pushed, resolved from local refs.
/// A concrete branch→remote push, an already-synced branch with nothing to push, or a
/// reason the repo is skipped — exactly one, never a combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    /// The current branch has unpushed commits and the remote its upstream tracks.
    Push { branch: String, remote: String },
    /// The branch has an upstream but no unpushed commits (`@{u}..HEAD` is empty), so a
    /// push would be a network no-op — the same "already synced" state `gtl status --all` reports.
    Synced { branch: String, remote: String },
    /// The repo cannot be pushed (no upstream, or detached HEAD); the string says why.
    Skip { reason: String },
}

/// One discovered repo: its path, a display label, and its resolved push destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoTarget {
    pub path: PathBuf,
    pub label: String,
    pub dest: Dest,
}

/// Read-only plan for the recursive `push -r` flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubreposPlan {
    /// Nothing to do; the string explains why (stderr, exit 1).
    Refused(String),
    /// Discovered repos with their resolved destinations.
    Ready(Vec<RepoTarget>),
}

/// What happened to one repo during an applied `push -r`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoOutcome {
    /// Pushed new commits.
    Pushed,
    /// Push ran but the remote was already current.
    UpToDate,
    /// Not pushed; the string says why (no upstream, detached HEAD).
    Skipped(String),
    /// Push failed; the string holds git's reason.
    Failed(String),
}

/// One repo's reported outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoReport {
    pub label: String,
    pub outcome: RepoOutcome,
}

/// Overall outcome of an applied recursive `push -r`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No repo failed (some may have been skipped or already current).
    Ok,
    /// Some repos pushed, some failed.
    Partial,
    /// At least one repo failed and none were pushed.
    Fail,
}

/// Applied result: a status, a human-readable detail block, and the per-repo reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushAllResult {
    pub status: Status,
    pub detail: String,
    pub reports: Vec<RepoReport>,
}
