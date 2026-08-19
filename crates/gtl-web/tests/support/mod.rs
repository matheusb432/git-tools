use std::error::Error;

use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitHead, GitRevision},
    paths::ProjectName,
    timestamps::MachineTimestamp,
    viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId},
};
use gtl_wire::viewer::{
    ViewerActiveView, ViewerCommandLine, ViewerCommitSelection, ViewerCommitSummary,
    ViewerDiffDensity, ViewerDiffLayout, ViewerFooter, ViewerRenderOptions, ViewerViewIdentity,
};

pub type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub fn viewer_commit_summary() -> TestResult<ViewerCommitSummary> {
    Ok(ViewerCommitSummary {
        id: CommitId::try_from("0123456789abcdef0123456789abcdef01234567")?,
        subject: "single commit".to_owned(),
        body: String::new(),
        committed_at: MachineTimestamp::try_from("2026-08-19T10:00:00Z")?,
        is_merge: false,
    })
}

pub fn viewer_active_view(commits: Vec<ViewerCommitSummary>) -> TestResult<ViewerActiveView> {
    Ok(ViewerActiveView {
        identity: ViewerViewIdentity {
            tab_id: ViewerTabId::try_new(1)?,
            range_generation: ViewerRangeGeneration::default(),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        },
        title: "single commit diff".to_owned(),
        repository_name: ProjectName::try_from("git-tools")?,
        branch: GitHead::Branch(BranchName::main()),
        upstream: GitRevision::main(),
        command: ViewerCommandLine {
            lead: "git diff ".to_owned(),
            range: "HEAD~1..HEAD".to_owned(),
            trail: String::new(),
        },
        files: Vec::new(),
        commits_label: "1 commit".to_owned(),
        commits,
        commit_selection: ViewerCommitSelection::None,
        footer: ViewerFooter {
            command: "gtl diff".to_owned(),
        },
        exclusions: None,
    })
}
