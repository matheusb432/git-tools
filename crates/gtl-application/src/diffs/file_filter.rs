use gtl_models::{diffs::ExtensionFilter, git::GitDiffSpec};

use super::FileDiff;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffFileFilter {
    pub(super) source: Option<GitDiffSpec>,
    pub(super) filter: ExtensionFilter,
    pub(super) hidden_files: Vec<FileDiff>,
}

impl DiffFileFilter {
    pub(crate) fn new(source: GitDiffSpec, filter: ExtensionFilter) -> Self {
        Self {
            source: Some(source),
            filter,
            hidden_files: Vec::new(),
        }
    }

    #[must_use]
    pub fn filter(&self) -> &ExtensionFilter {
        &self.filter
    }

    pub(crate) fn hidden_files(&self) -> &[FileDiff] {
        &self.hidden_files
    }
}
