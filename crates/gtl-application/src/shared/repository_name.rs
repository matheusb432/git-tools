//! Repository-name policy shared by application operations and process roots.

use gtl_models::paths::{ProjectName, RepositoryRoot};

/// Returns the project identity derived from a resolved repository root.
#[must_use]
pub fn from_root(root: &RepositoryRoot) -> ProjectName {
    root.project_name()
}

#[cfg(test)]
mod tests {
    use super::from_root;
    use crate::utils::repository_root;

    #[test]
    fn uses_the_final_component_or_a_fallback() {
        assert_eq!(from_root(&repository_root("/work/api")).as_str(), "api");
        assert_eq!(from_root(&repository_root("/")).as_str(), "repo");
    }
}
