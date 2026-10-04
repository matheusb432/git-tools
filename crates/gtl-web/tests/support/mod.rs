use std::error::Error;

use gtl_models::{
    diffs::CommitId,
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
        modified_files: false,
        source: gtl_wire::viewer::ViewerViewSource::Repository,
        row_source: gtl_wire::viewer::ViewerRowSourceState::Ready,
        content_id: gtl_wire::viewer::ViewerRowContentId::from_digest([0; 32]),
        identity: ViewerViewIdentity {
            tab_id: ViewerTabId::try_new(1)?,
            range_generation: ViewerRangeGeneration::default(),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        },
        title: gtl_models::diffs::DiffViewTitle::Diff,
        command: ViewerCommandLine {
            lead: "git diff ".to_owned(),
            range: "HEAD~1..HEAD".to_owned(),
            trail: String::new(),
        },
        files: Vec::new(),
        commit_count: commits.len(),
        commits,
        commit_selection: ViewerCommitSelection::None,
        footer: ViewerFooter {
            command: "gtl diff".to_owned(),
        },
        extension_filter: None,
        changes_since: None,
    })
}
