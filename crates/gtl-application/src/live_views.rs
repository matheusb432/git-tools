//! The live-views feature: persisted repo views the app renders on demand.

pub mod delete_live_viewer_tab;
pub mod list_live_views;
mod persistence;
pub mod probe_source;
pub mod save_live_view;

use gtl_models::live_views::LiveSource;
pub use persistence::LiveViewRecord;

use crate::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

/// Builds the unpushed diff recipe represented by a saved live view.
pub fn recipe_for_record(record: &LiveViewRecord) -> Recipe {
    let LiveSource::LocalRepo { path } = &record.source;
    Recipe {
        source: RecipeSource::LocalRepo(path.clone()),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: Some(record.display_name.clone()),
    }
}
