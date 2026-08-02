use std::path::{Path, PathBuf};

pub(super) fn root(repo_root: &Path) -> PathBuf {
    repo_root.join(".artifacts").join("gtl")
}
