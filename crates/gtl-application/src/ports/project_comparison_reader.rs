use gtl_models::{paths::RepositoryRoot, projects::comparison::ComparisonBranch};

pub trait ProjectComparisonReader {
    fn comparison_branch(&self, path: &RepositoryRoot) -> anyhow::Result<Option<ComparisonBranch>>;
}

impl ProjectComparisonReader for std::collections::BTreeMap<RepositoryRoot, ComparisonBranch> {
    fn comparison_branch(&self, path: &RepositoryRoot) -> anyhow::Result<Option<ComparisonBranch>> {
        Ok(self.get(path).cloned())
    }
}
