//! Repository-name policy shared by application operations and process roots.

use std::path::Path;

/// Returns the final path component, or `"repo"` when the path has no name.
pub fn from_path(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::from_path;

    #[test]
    fn uses_the_final_component_or_a_fallback() {
        assert_eq!(from_path(Path::new("/work/api")), "api");
        assert_eq!(from_path(Path::new("/")), "repo");
    }
}
