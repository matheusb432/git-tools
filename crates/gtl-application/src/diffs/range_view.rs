use gtl_models::{diffs::DiffViewTitle, git::GitDiffSpec};

use super::view::{Cmd, Foot};

pub(super) const TITLE_MERGE_DIFF: &str = "merge-diff";
const GIT_DIFF_LEAD: &str = "git diff ";

pub(super) struct RangeView {
    pub(super) title: DiffViewTitle,
    pub(super) cmd: Cmd,
    pub(super) foot: Foot,
}

impl RangeView {
    pub(super) fn new(spec: &GitDiffSpec, title: DiffViewTitle) -> Self {
        let range = spec.to_string();
        Self {
            title,
            cmd: Cmd {
                lead: GIT_DIFF_LEAD.to_string(),
                range: range.clone(),
                trail: String::new(),
            },
            foot: Foot {
                cmd: format!("{GIT_DIFF_LEAD}{range}"),
            },
        }
    }
}
