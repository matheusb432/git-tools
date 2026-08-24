use super::PlacedArtifact;
use crate::{
    diffs::{
        render_diff::RenderDiff, render_diff_subrepos::RenderDiffSubrepos,
        render_merge_diff::RenderMergeDiff,
    },
    projects::render_project_diff::RenderProjectDiff,
    recipes::RecipeBatch,
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq)]
pub enum DiffRenderRequest {
    Diff(RenderDiff),
    MergeDiff(RenderMergeDiff),
    Subrepos(RenderDiffSubrepos),
    Projects(RenderProjectDiff),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRenderOutcome {
    Rendered(PlacedArtifact),
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffRenderResponse {
    pub outcome: DiffRenderOutcome,
    pub notes: Vec<Note>,
}

pub trait DiffViewerClient: Clone + Send + Sync + 'static {
    fn forward(&self, batch: &RecipeBatch) -> anyhow::Result<()>;

    fn render(&self, request: &DiffRenderRequest) -> anyhow::Result<DiffRenderResponse>;
}
