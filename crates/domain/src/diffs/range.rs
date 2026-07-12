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

/// Build [`Ranges`] whose diff/log run over `range` verbatim while keeping
/// `mode`'s semantic labels — the pinned-target companion to [`ranges`].
/// `Hash` has no pinned form (it diffs the working tree) and maps to the
/// exact-range labels defensively.
pub fn ranges_over(range: &str, mode: Mode) -> Ranges {
    match mode {
        Mode::Unpushed => Ranges {
            diff_args: vec!["diff".to_string(), range.to_string()],
            diff_range: range.to_string(),
            log_range: range.to_string(),
            title: "diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: range.to_string(),
                trail: String::new(),
            },
            commits_label: "# unpushed commits".to_string(),
            foot: Foot {
                cmd: format!("git diff {range}"),
                note: "# unpushed work — read-only preview".to_string(),
            },
        },
        Mode::Merge => Ranges {
            diff_args: vec!["diff".to_string(), range.to_string()],
            diff_range: range.to_string(),
            log_range: range.to_string(),
            title: "merge-diff".to_string(),
            cmd: Cmd {
                lead: "git diff ".to_string(),
                range: range.to_string(),
                trail: String::new(),
            },
            commits_label: "# commits to merge".to_string(),
            foot: Foot {
                cmd: format!("git diff {range}"),
                note: "# merge preview — read-only".to_string(),
            },
        },
        Mode::Hash | Mode::ExactRange => ranges(range, Mode::ExactRange),
    }
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

    #[test]
    fn ranges_over_keeps_unpushed_labels_over_the_exact_range() {
        let exact = ranges_over("aaa..bbb", Mode::Unpushed);
        assert_eq!(exact.diff_args, vec!["diff", "aaa..bbb"]);
        assert_eq!(exact.diff_range, "aaa..bbb");
        assert_eq!(exact.log_range, "aaa..bbb");
        assert_eq!(exact.title, "diff");
        assert_eq!(exact.commits_label, "# unpushed commits");
        assert_eq!(exact.foot.cmd, "git diff aaa..bbb");

        // Ensure labels don't drift from the symbolic builder
        let symbolic = ranges("origin/main", Mode::Unpushed);
        assert_eq!(exact.title, symbolic.title);
        assert_eq!(exact.commits_label, symbolic.commits_label);
        assert_eq!(exact.foot.note, symbolic.foot.note);
        assert_eq!(exact.cmd.lead, symbolic.cmd.lead);
    }

    #[test]
    fn ranges_over_keeps_merge_labels_over_the_exact_two_dot_range() {
        let exact = ranges_over("aaa..bbb", Mode::Merge);
        assert_eq!(exact.diff_args, vec!["diff", "aaa..bbb"]);
        assert_eq!(exact.log_range, "aaa..bbb");
        assert_eq!(exact.title, "merge-diff");
        assert_eq!(exact.commits_label, "# commits to merge");

        // Ensure labels don't drift from the symbolic builder
        let symbolic = ranges("main", Mode::Merge);
        assert_eq!(exact.title, symbolic.title);
        assert_eq!(exact.commits_label, symbolic.commits_label);
        assert_eq!(exact.foot.note, symbolic.foot.note);
        assert_eq!(exact.cmd.lead, symbolic.cmd.lead);
    }

    #[test]
    fn ranges_over_exact_range_matches_the_symbolic_builder() {
        assert_eq!(
            ranges_over("a..b", Mode::ExactRange).diff_range,
            ranges("a..b", Mode::ExactRange).diff_range
        );
        assert_eq!(
            ranges_over("a..b", Mode::ExactRange).commits_label,
            "# commits in range"
        );
    }
}
