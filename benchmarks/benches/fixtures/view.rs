use gtl_application::diffs::{Cmd, FileDiff, Foot, View};

const FIXTURE_LINE_COUNT: usize = 45_000;

pub(super) fn large_view() -> View {
    view_with_lines(FIXTURE_LINE_COUNT)
}

pub(super) fn view_with_lines(line_count: usize) -> View {
    let mut lines = Vec::with_capacity(line_count);
    lines.push(format!("@@ -1,{line_count} +1,{line_count} @@"));
    lines.extend(
        (1..line_count).map(|line| format!(" line {line:05}: deterministic benchmark payload")),
    );

    View {
        exclusions: None,
        repo_name: "benchmark".into(),
        repo_root: "/fixtures/benchmark".into(),
        branch: "main".into(),
        upstream: "origin/main".into(),
        commits: vec![],
        files: vec![FileDiff {
            path: "src/large.rs".into(),
            added: 0,
            removed: 0,
            full_lines: Some(lines.clone()),
            lines,
        }],
        title: "Large diff".into(),
        cmd: Cmd {
            lead: "git diff ".into(),
            range: "origin/main..HEAD".into(),
            trail: String::new(),
        },
        commits_label: "0 commits".into(),
        foot: Foot {
            cmd: "git diff origin/main..HEAD".into(),
            note: "benchmark fixture".into(),
        },
    }
}
