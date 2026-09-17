use gtl_models::{diffs::ExcludedExtensions, paths::ProjectName};
use serde::{Deserialize, Serialize};

use super::{FieldUpdate, ViewerTabId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerFileFilters {
    pub excluded: ExcludedExtensions,
    pub extensions: Vec<String>,
    pub project: Option<ProjectName>,
    pub saved: Option<ExcludedExtensions>,
    pub defaults: ExcludedExtensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetViewerFileFilters {
    pub tab_id: ViewerTabId,
    pub exclusions: FieldUpdate<ExcludedExtensions>,
    pub expected: Option<ExcludedExtensions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateDiffExclusions {
    pub project: Option<ProjectName>,
    pub extensions: FieldUpdate<ExcludedExtensions>,
    pub expected: Option<ExcludedExtensions>,
}
