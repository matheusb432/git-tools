//! Data shape for one managed repo's classified status. A dumb carrier — the
//! `managed::status_repos` application slice builds it; the CLI formats it.

use serde::Serialize;

/// One repo's status facts plus its classified `state`/`detail` summary, exactly
/// as `status`/`status -r`/`status --all` report them (the `--json` wire shape).
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
