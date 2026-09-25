use gtl_models::diffs::ExtensionFilter;
use serde::{Deserialize, Serialize};

use super::ViewerTabId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFileFilters {
    pub filter: ExtensionFilter,
    /// Lists the extensions of every changed file, including hidden ones.
    pub extensions: Vec<String>,
}

/// Applies `filter` to one tab and saves it for the tab's repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetViewerFileFilters {
    pub tab_id: ViewerTabId,
    pub filter: ExtensionFilter,
}
