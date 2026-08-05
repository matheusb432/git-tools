use std::path::PathBuf;

use crate::{
    diffs::{
        DiffTarget, PinnedRange, render_diff::RenderDiff, render_diff_all::RenderDiffAll,
        render_diff_subrepos::RenderDiffSubrepos, render_merge_diff::RenderMergeDiff,
    },
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffViewerRecipe {
    pub source: PathBuf,
    pub operation: DiffViewerRecipeOperation,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffViewerRecipeOperation {
    Diff(DiffTarget),
    MergeDiff {
        base: Option<String>,
        pinned: Option<PinnedRange>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffViewerBatch {
    pub batch_id: String,
    pub recipes: Vec<DiffViewerRecipe>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiffRenderRequest {
    Diff(RenderDiff),
    MergeDiff(RenderMergeDiff),
    Subrepos(RenderDiffSubrepos),
    ManagedAll(RenderDiffAll),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRenderOutcome {
    Rendered(PathBuf),
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffRenderResponse {
    pub outcome: DiffRenderOutcome,
    pub notes: Vec<Note>,
}

pub trait DiffViewerClient: Clone + Send + Sync + 'static {
    fn forward(&self, batch: &DiffViewerBatch) -> anyhow::Result<()>;

    fn render(&self, request: &DiffRenderRequest) -> anyhow::Result<DiffRenderResponse>;
}
