use super::view::{Cmd, Foot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Unpushed,
    Hash,
    Merge,
    ExactRange,
}

#[derive(Debug, Clone)]
pub struct Ranges {
    pub diff_args: Vec<String>,
    pub diff_range: String,
    pub log_range: String,
    pub title: String,
    pub cmd: Cmd,
    pub commits_label: String,
    pub foot: Foot,
}

pub fn ranges(base: &str, mode: Mode) -> Ranges {
    match mode {
        Mode::Unpushed => {
            let range = format!("{base}..HEAD");
            Ranges {
                diff_args: vec!["diff".to_string(), range.clone()],
                diff_range: range.clone(),
                log_range: range.clone(),
                title: "diff".to_string(),
                cmd: Cmd {
                    lead: "git diff ".to_string(),
                    range: range.clone(),
                    trail: String::new(),
                },
                commits_label: "# unpushed commits".to_string(),
                foot: Foot {
                    cmd: format!("git diff {range}"),
                    note: "# unpushed work — read-only preview".to_string(),
                },
            }
        }
        Mode::Hash => {
            let log_range = format!("{base}..HEAD");
            Ranges {
                diff_args: vec!["diff".to_string(), base.to_string()],
                diff_range: base.to_string(),
                log_range,
                title: "diff".to_string(),
                cmd: Cmd {
                    lead: "git diff ".to_string(),
                    range: base.to_string(),
                    trail: String::new(),
                },
                commits_label: format!("# commits since {base}"),
                foot: Foot {
                    cmd: format!("git diff {base}"),
                    note: "# base → working tree — read-only preview".to_string(),
                },
            }
        }
        Mode::Merge => {
            let diff_range = format!("{base}...HEAD");
            let log_range = format!("{base}..HEAD");
            Ranges {
                diff_args: vec!["diff".to_string(), diff_range.clone()],
                diff_range: diff_range.clone(),
                log_range,
                title: "merge-diff".to_string(),
                cmd: Cmd {
                    lead: "git diff ".to_string(),
                    range: diff_range.clone(),
                    trail: String::new(),
                },
                commits_label: "# commits to merge".to_string(),
                foot: Foot {
                    cmd: format!("git diff {diff_range}"),
                    note: "# merge preview — read-only".to_string(),
                },
            }
        }
        Mode::ExactRange => Ranges {
            diff_args: vec!["diff".to_string(), base.to_string()],
            diff_range: base.to_string(),
            log_range: base.to_string(),
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: base.to_string(),
                trail: String::new(),
            },
            commits_label: "# commits in range".to_string(),
            foot: Foot {
                cmd: format!("git diff {base}"),
                note: "# commit range — read-only preview".to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_mode_uses_three_dot_diff_and_two_dot_log() {
        let ranges = ranges("main", Mode::Merge);

        assert_eq!(ranges.diff_args, vec!["diff", "main...HEAD"]);
        assert_eq!(ranges.diff_range, "main...HEAD");
        assert_eq!(ranges.log_range, "main..HEAD");
    }

    #[test]
    fn hash_mode_diffs_base_to_working_tree_and_logs_base_to_head() {
        let ranges = ranges("abc123", Mode::Hash);

        assert_eq!(ranges.diff_args, vec!["diff", "abc123"]);
        assert_eq!(ranges.diff_range, "abc123");
        assert_eq!(ranges.log_range, "abc123..HEAD");
        assert_eq!(ranges.title, "diff");
        assert_eq!(ranges.commits_label, "# commits since abc123");
        assert_eq!(ranges.foot.cmd, "git diff abc123");
    }

    #[test]
    fn unpushed_mode_uses_upstream_to_head_for_diff_and_log() {
        let ranges = ranges("origin/main", Mode::Unpushed);

        assert_eq!(ranges.diff_args, vec!["diff", "origin/main..HEAD"]);
        assert_eq!(ranges.diff_range, "origin/main..HEAD");
        assert_eq!(ranges.log_range, "origin/main..HEAD");
        assert_eq!(ranges.title, "diff");
        assert_eq!(ranges.commits_label, "# unpushed commits");
        assert_eq!(ranges.foot.cmd, "git diff origin/main..HEAD");
    }

    #[test]
    fn exact_range_mode_uses_range_for_diff_and_log() {
        let ranges = ranges("abc123..def456", Mode::ExactRange);

        assert_eq!(ranges.diff_args, vec!["diff", "abc123..def456"]);
        assert_eq!(ranges.diff_range, "abc123..def456");
        assert_eq!(ranges.log_range, "abc123..def456");
        assert_eq!(ranges.commits_label, "# commits in range");
    }

    #[test]
    fn merge_mode_labels_three_dot_merge_preview() {
        let ranges = ranges("main", Mode::Merge);

        assert_eq!(ranges.title, "merge-diff");
        assert_eq!(ranges.commits_label, "# commits to merge");
        assert_eq!(ranges.foot.cmd, "git diff main...HEAD");
    }
}
