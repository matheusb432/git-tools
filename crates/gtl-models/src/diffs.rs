//! The diffs feature's models model.

mod commit;
mod counts;
mod extension_filter;
mod kind;
mod review_content_id;
mod text;
mod view_title;

pub use commit::{
    Commit, CommitId, CommitIdAbbreviation, CommitIdError, CommitTimeRange, CommitTimeRangeError,
    PinnedRange,
};
pub use counts::DiffLineCount;
pub use extension_filter::{
    AppliedExtensionFilter, ExtensionFilter, ExtensionFilterMode, ExtensionSelection,
    FileExtensions, ParseExtensionFilterModeError,
};
pub use kind::DiffKind;
pub use review_content_id::{DiffFileReviewReference, DiffReviewContentId, DiffReviewScope};
pub use text::{DIFF_TEXT_BYTES_MAX, DiffText, DiffTextError, DiffTextId, DiffTextIdError};
pub use view_title::DiffViewTitle;
