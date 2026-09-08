use gtl_models::git::GitDiffSpec;

use super::view::{Cmd, Foot};

const TITLE_DIFF: &str = "diff";
pub(super) const TITLE_MERGE_DIFF: &str = "merge-diff";
const GIT_DIFF_LEAD: &str = "git diff ";
pub(super) const LABEL_UNPUSHED_COMMITS: &str = "# unpushed commits";
pub(super) const LABEL_COMMITS_IN_RANGE: &str = "# commits in range";
const LABEL_COMMITS_TO_MERGE: &str = "# commits to merge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RangePresentation {
    Unpushed,
    Branch,
    WorkingTree,
    Exact,
    Merge,
}

pub(super) struct RangeView {
    pub(super) title: String,
    pub(super) cmd: Cmd,
    pub(super) commits_label: String,
    pub(super) foot: Foot,
}

impl RangeView {
    pub(super) fn new(spec: &GitDiffSpec, presentation: RangePresentation) -> Self {
        let range = spec.to_string();
        let (title, commits_label) = match presentation {
            RangePresentation::Branch => (TITLE_DIFF, "# branch changes".to_string()),
            RangePresentation::Unpushed => (TITLE_DIFF, LABEL_UNPUSHED_COMMITS.to_string()),
            RangePresentation::WorkingTree => (TITLE_DIFF, format!("# commits since {range}")),
            RangePresentation::Exact => (TITLE_DIFF, LABEL_COMMITS_IN_RANGE.to_string()),
            RangePresentation::Merge => (TITLE_MERGE_DIFF, LABEL_COMMITS_TO_MERGE.to_string()),
        };

        Self {
            title: title.to_string(),
            cmd: Cmd {
                lead: GIT_DIFF_LEAD.to_string(),
                range: range.clone(),
                trail: String::new(),
            },
            commits_label,
            foot: Foot {
                cmd: format!("{GIT_DIFF_LEAD}{range}"),
            },
        }
    }
}
