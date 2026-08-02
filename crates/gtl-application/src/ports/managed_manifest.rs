use std::{future::Future, path::Path};

use gtl_models::managed::ManagedRepo;

/// Parses an already-resolved manifest file into the managed repo list. *Where*
/// the file lives (env var, upward search, `sample_project`, home-dir default) stays a
/// CLI concern - it depends on the caller's shell cwd, same reasoning as
/// merge-diff's cwd-absolute-resolution rule.
pub trait ManagedManifest: Clone + Send + Sync + 'static {
    fn load(
        &self,
        repos_file: &Path,
        home_dir: &Path,
    ) -> impl Future<Output = anyhow::Result<Vec<ManagedRepo>>> + Send;
}
