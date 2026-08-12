use std::cmp::Ordering;

use gtl_models::tags::Tag;

pub mod add;
pub mod add_and_push;
pub mod bump_tag;
pub mod dry_run_tag_bump;
mod git_command_error;
pub mod label;
pub mod list;
mod outcome;
pub mod push;
mod refs;
mod version;

pub use dry_run_tag_bump::{DryRunTagBump, DryRunTagBumpOk, TagBumpPreview};
pub use list::{ListTagsOk, TagGroup};
pub use outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress, TagRemotePushProgress};
pub use version::BumpLevel;

fn compare_tags(left: &Tag, right: &Tag) -> Ordering {
    left.created_at()
        .cmp(&right.created_at())
        .then_with(|| natord::compare(left.name(), right.name()))
}
