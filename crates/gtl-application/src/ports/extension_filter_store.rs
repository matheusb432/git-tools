use std::collections::BTreeMap;

use gtl_models::{diffs::ExtensionFilter, paths::RepositoryRoot};

/// Reads the extension filter saved for one repository root.
pub trait ExtensionFilterReader {
    /// Returns the saved filter, or the inactive default when the repository has none.
    fn extension_filter(&self, repository: &RepositoryRoot) -> anyhow::Result<ExtensionFilter>;

    /// Returns the filter for a diff without a repository, which never has a saved filter.
    fn text_extension_filter(&self) -> ExtensionFilter {
        ExtensionFilter::default()
    }
}

/// Saves the extension filter for one repository root.
pub trait ExtensionFilterWriter {
    /// Replaces the saved filter; saving an inactive filter forgets it.
    fn save_extension_filter(
        &self,
        repository: &RepositoryRoot,
        filter: &ExtensionFilter,
    ) -> anyhow::Result<()>;
}

impl ExtensionFilterReader for BTreeMap<RepositoryRoot, ExtensionFilter> {
    fn extension_filter(&self, repository: &RepositoryRoot) -> anyhow::Result<ExtensionFilter> {
        Ok(self.get(repository).cloned().unwrap_or_default())
    }
}
