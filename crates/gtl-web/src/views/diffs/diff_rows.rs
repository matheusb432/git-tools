mod content;
mod split;
mod unified;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderTone {
    Meta,
    Hunk,
}

pub(crate) use split::SplitDiffRowBatch;
pub(crate) use unified::UnifiedDiffRowBatch;
