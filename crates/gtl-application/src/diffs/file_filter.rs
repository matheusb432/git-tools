use gtl_models::{diffs::ExcludedExtensions, git::GitDiffSpec};

use super::FileDiff;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffFileFilter {
    pub(super) source: Option<GitDiffSpec>,
    pub(super) excluded: ExcludedExtensions,
    pub(super) hidden_files: Vec<FileDiff>,
}

impl DiffFileFilter {
    pub(crate) fn new(source: GitDiffSpec, excluded: ExcludedExtensions) -> Self {
        Self {
            source: Some(source),
            excluded,
            hidden_files: Vec::new(),
        }
    }

    #[must_use]
    pub fn excluded(&self) -> &ExcludedExtensions {
        &self.excluded
    }

    pub(crate) fn hidden_files(&self) -> &[FileDiff] {
        &self.hidden_files
    }
}
