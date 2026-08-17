use std::cmp::Ordering;

use gtl_models::tags::Tag;

pub mod add_and_push_tag;
pub mod add_tag;
pub mod bump_tag;
pub mod dry_run_tag_bump;
mod git_command_error;
pub mod label_tag;
pub mod list_tags;
mod outcome;
pub mod push_tags;
mod refs;
mod version;

pub use dry_run_tag_bump::{DryRunTagBump, DryRunTagBumpOk, TagBumpPreview};
pub use list_tags::{ListTagsOk, TagGroup};
pub use outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress, TagRemotePushProgress};
pub use version::BumpLevel;

fn compare_tags(left: &Tag, right: &Tag) -> Ordering {
    left.created_at()
        .cmp(&right.created_at())
        .then_with(|| natord::compare(left.name().as_ref(), right.name().as_ref()))
}
