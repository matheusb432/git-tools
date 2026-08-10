use super::view::{Cmd, Foot};

const TITLE_DIFF: &str = "diff";
pub(crate) const TITLE_MERGE_DIFF: &str = "merge-diff";
const GIT_DIFF_LEAD: &str = "git diff ";
pub(crate) const LABEL_UNPUSHED_COMMITS: &str = "# unpushed commits";
pub(crate) const LABEL_COMMITS_IN_RANGE: &str = "# commits in range";
const LABEL_COMMITS_TO_MERGE: &str = "# commits to merge";
const NOTE_UNPUSHED_WORK: &str = "# unpushed work";
pub(crate) const NOTE_WORKING_TREE: &str = "# base → working tree";
const NOTE_COMMIT_RANGE: &str = "# commit range";
const NOTE_MERGE_DIFF: &str = "# merge diff";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangePresentation {
    Unpushed,
    WorkingTree,
    Exact,
    Merge,
}

pub(crate) struct RangeView {
    pub(crate) title: String,
    pub(crate) cmd: Cmd,
    pub(crate) commits_label: String,
    pub(crate) foot: Foot,
}

impl RangeView {
    pub(crate) fn new(range: &str, presentation: RangePresentation) -> Self {
        let (title, commits_label, note) = match presentation {
            RangePresentation::Unpushed => (
                TITLE_DIFF,
                LABEL_UNPUSHED_COMMITS.to_string(),
                NOTE_UNPUSHED_WORK,
            ),
            RangePresentation::WorkingTree => (
                TITLE_DIFF,
                format!("# commits since {range}"),
                NOTE_WORKING_TREE,
            ),
            RangePresentation::Exact => (
                TITLE_DIFF,
                LABEL_COMMITS_IN_RANGE.to_string(),
                NOTE_COMMIT_RANGE,
            ),
            RangePresentation::Merge => (
                TITLE_MERGE_DIFF,
                LABEL_COMMITS_TO_MERGE.to_string(),
                NOTE_MERGE_DIFF,
            ),
        };

        Self {
            title: title.to_string(),
            cmd: Cmd {
                lead: GIT_DIFF_LEAD.to_string(),
                range: range.to_string(),
                trail: String::new(),
            },
            commits_label,
            foot: Foot {
                cmd: format!("{GIT_DIFF_LEAD}{range}"),
                note: note.to_string(),
            },
        }
    }
}
