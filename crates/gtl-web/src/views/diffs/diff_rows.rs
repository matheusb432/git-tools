mod content;
mod split;
mod unified;

const HEADER_CODE_CLASSES: &str = "min-w-0 border-0 bg-transparent px-3 text-sm text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 print:text-[#111]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderTone {
    Meta,
    Hunk,
}

pub(crate) use split::SplitDiffRowBatch;
pub(crate) use unified::UnifiedDiffRowBatch;
