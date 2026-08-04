//! Parses an already-resolved sample_project project manifest into GTL's managed Git repositories.

use std::path::Path;

use gtl_application::ports::ManagedManifest;
use gtl_models::managed::ManagedRepo;
use serde::Deserialize;

#[derive(Debug, Default, Clone, Copy)]
pub struct TokioManagedManifest;

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    project: Vec<ProjectEntry>,
}

#[derive(Deserialize)]
struct ProjectEntry {
    path: String,
    #[serde(default)]
    remote: String,
    #[serde(default = "default_true")]
    managed: bool,
}

fn default_true() -> bool {
    true
}

fn parse(raw: &str, home_dir: &Path) -> anyhow::Result<Vec<ManagedRepo>> {
    let manifest: Manifest = toml::from_str(raw)
        .map_err(|e| anyhow::anyhow!("parsing managed-repos manifest (projects.toml): {e}"))?;
    let mut repos = Vec::new();
    for entry in manifest.project {
        let local = entry.path.trim();
        if local.is_empty() || !entry.managed {
            continue;
        }
        repos.push(ManagedRepo {
            name: local.to_string(),
            path: home_dir.join(local),
            remote: entry.remote.trim().to_string(),
        });
    }
    Ok(repos)
}

impl ManagedManifest for TokioManagedManifest {
    async fn load(&self, repos_file: &Path, home_dir: &Path) -> anyhow::Result<Vec<ManagedRepo>> {
        let raw = tokio::fs::read_to_string(repos_file).await.map_err(|e| {
            anyhow::anyhow!(
                "managed-repos manifest not found: {}: {e}",
                repos_file.display()
            )
        })?;
        parse(&raw, home_dir)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parses_toml_manifest_with_comments_and_missing_remote() {
        let repos = parse(
            "# comment\n\
             [[project]]\npath = \"self/repo-b\"\nremote = \"https://example.invalid/cfg.git\"\n\n\
             [[project]]\npath = \"tools/git-tools\"\n",
            Path::new("/home/me"),
        )
        .unwrap();

        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "self/repo-b");
        assert_eq!(repos[0].path, Path::new("/home/me").join("self/repo-b"));
        assert_eq!(repos[0].remote, "https://example.invalid/cfg.git");
        assert_eq!(repos[1].remote, "");
    }

    #[test]
    fn skips_paused_repos() {
        let raw = "[[project]]\npath='self/cfg'\nremote='git@x:c.git'\n\
                   [[project]]\npath='work/sample_project'\nremote='git@x:i.git'\nmanaged=false\n";
        let repos = parse(raw, Path::new("/home/u")).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "self/cfg");
    }

    #[tokio::test]
    async fn load_reads_and_parses_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("projects.toml");
        std::fs::write(&manifest_path, "[[project]]\npath = \"a/b\"\n").unwrap();

        let repos = TokioManagedManifest
            .load(&manifest_path, dir.path())
            .await
            .unwrap();

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].path, dir.path().join("a/b"));
    }

    #[tokio::test]
    async fn load_errors_when_the_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let error = TokioManagedManifest
            .load(&dir.path().join("nope.toml"), dir.path())
            .await
            .expect_err("missing file should error");
        assert!(error.to_string().contains("manifest not found"));
    }
}
