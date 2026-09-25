use super::{ExtensionFilterReader, ProjectComparisonReader};

/// Reads every preference `gtl.db` saves per repository: its comparison branch and extension
/// filter.
pub trait RepositoryPreferenceReader: ExtensionFilterReader + ProjectComparisonReader {}

impl<T: ExtensionFilterReader + ProjectComparisonReader> RepositoryPreferenceReader for T {}
