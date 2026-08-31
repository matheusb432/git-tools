use gtl_application::diffs::{Cmd, FileDiff, Foot, View};
use gtl_benchmarks::require;
use gtl_models::git::{BranchName, GitHead, GitRevision, RemoteName};

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
        repo_name: require(
            gtl_models::paths::ProjectName::try_from("benchmark"),
            "creating the benchmark project name",
        ),
        repo_root: require(
            gtl_models::paths::RepositoryRoot::try_new("/fixtures/benchmark".into()),
            "creating the benchmark repository root",
        ),
        branch: GitHead::Branch(BranchName::main()),
        upstream: GitRevision::remote_tracking(&RemoteName::origin(), &BranchName::main()),
        commits: vec![],
        files: vec![FileDiff {
            path: require(
                gtl_models::paths::RepositoryRelativePath::try_new("src/large.rs".into()),
                "creating the benchmark file path",
            ),
            added: gtl_models::diffs::DiffLineCount::default(),
            removed: gtl_models::diffs::DiffLineCount::default(),
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
        },
        full_context: gtl_application::diffs::FullContextDiffState::Loaded,
    }
}
