//! Loading and resolving the `repos.toml` managed-repos manifest.

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use serde::Deserialize;

use super::{ManagedOptions, ManagedRepo};

/// The `repos.toml` document: an array of `[[repo]]` tables. Only `path` + `remote` are
/// read here; sample_project owns the other fields (`code`/`slug`/`color`).
#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    repo: Vec<RepoEntry>,
}

#[derive(Deserialize)]
struct RepoEntry {
    path: String,
    #[serde(default)]
    remote: String,
    #[serde(default = "default_true")]
    managed: bool,
}

/// serde default for `RepoEntry::managed` — repos are in-scope unless they opt out (CFG-0185).
fn default_true() -> bool {
    true
}

fn parse_manifest(raw: &str, home_dir: &Path) -> anyhow::Result<Vec<ManagedRepo>> {
    let manifest: Manifest =
        toml::from_str(raw).context("parsing managed-repos manifest (repos.toml)")?;
    let mut repos = Vec::new();

    for entry in manifest.repo {
        let local = entry.path.trim();
        if local.is_empty() {
            continue;
        }
        if !entry.managed {
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

fn resolve_home_dir(override_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = override_dir {
        return dir.to_path_buf();
    }
    home_dir_from_env().unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_repos_file(override_file: Option<&Path>) -> anyhow::Result<PathBuf> {
    let env_file = std::env::var_os("GIT_TOOLS_MANAGED_REPOS_FILE").map(PathBuf::from);
    let current_dir = std::env::current_dir()?;
    let home_dir = home_dir_from_env();

    resolve_repos_file_from_sources(
        override_file,
        env_file.as_deref(),
        &current_dir,
        home_dir.as_deref(),
    )
}

fn home_dir_from_env() -> Option<PathBuf> {
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        return Some(PathBuf::from(userprofile));
    }
    std::env::var_os("HOME").map(PathBuf::from)
}

fn resolve_repos_file_from_sources(
    override_file: Option<&Path>,
    env_file: Option<&Path>,
    current_dir: &Path,
    home_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if let Some(file) = override_file {
        return Ok(file.to_path_buf());
    }
    if let Some(file) = env_file {
        return Ok(file.to_path_buf());
    }
    if let Some(found) = find_upward_config(current_dir) {
        return Ok(found);
    }
    if let Some(home_dir) = home_dir {
        let candidate = home_default_repos_file(home_dir);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let home_hint = home_dir
        .map(home_default_repos_file)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "$HOME/self/sample_project/config/provisioning/linux/repos.toml".into());
    Err(anyhow!(
        "managed-repos manifest not found; pass --repos-file, set GIT_TOOLS_MANAGED_REPOS_FILE, run from a sample_project checkout, or install sample_project at {home_hint}"
    ))
}

fn find_upward_config(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir
            .join("config")
            .join("provisioning")
            .join("linux")
            .join("repos.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn home_default_repos_file(home_dir: &Path) -> PathBuf {
    home_dir
        .join("self")
        .join("sample_project")
        .join("config")
        .join("provisioning")
        .join("linux")
        .join("repos.toml")
}

pub fn load_repos(options: &ManagedOptions) -> anyhow::Result<Vec<ManagedRepo>> {
    let repos_file = resolve_repos_file(options.repos_file.as_deref())?;
    let home_dir = resolve_home_dir(options.home_dir.as_deref());
    let raw = std::fs::read_to_string(&repos_file)
        .with_context(|| format!("managed-repos manifest not found: {}", repos_file.display()))?;
    parse_manifest(&raw, &home_dir)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::commands::managed::test_support::{ManagedFixture, touch};

    #[test]
    fn parses_toml_manifest_with_comments_and_missing_remote() {
        let repos = parse_manifest(
            "# comment\n\
             [[repo]]\npath = \"self/repo-b\"\nremote = \"https://example.invalid/cfg.git\"\ncode = \"CFG\"\n\n\
             [[repo]]\npath = \"tools/git-tools\"\n",
            Path::new("/home/me"),
        )
        .unwrap();

        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "self/repo-b");
        assert_eq!(repos[0].path, Path::new("/home/me").join("self/repo-b"));
        assert_eq!(repos[0].remote, "https://example.invalid/cfg.git");
        assert_eq!(repos[1].name, "tools/git-tools");
        assert_eq!(repos[1].remote, "");
    }

    #[test]
    fn resolve_repos_file_sources_respect_precedence() {
        let fixture = ManagedFixture::new("resolve-precedence");
        let explicit = fixture.root.join("explicit.txt");
        let env_file = fixture.root.join("env.txt");
        let cwd = fixture.root.join("workspace").join("repo");
        let upward = fixture
            .root
            .join("workspace/config/provisioning/linux/repos.toml");
        let home_default = fixture
            .home
            .join("self/sample_project/config/provisioning/linux/repos.toml");
        touch(&explicit);
        touch(&env_file);
        touch(&upward);
        touch(&home_default);

        assert_eq!(
            resolve_repos_file_from_sources(
                Some(&explicit),
                Some(&env_file),
                &cwd,
                Some(&fixture.home),
            )
            .unwrap(),
            explicit
        );
        assert_eq!(
            resolve_repos_file_from_sources(None, Some(&env_file), &cwd, Some(&fixture.home))
                .unwrap(),
            env_file
        );
        assert_eq!(
            resolve_repos_file_from_sources(None, None, &cwd, Some(&fixture.home)).unwrap(),
            upward
        );
    }

    #[test]
    fn resolve_repos_file_uses_home_default_after_upward_search_misses() {
        let fixture = ManagedFixture::new("resolve-home-default");
        let cwd = fixture.root.join("elsewhere").join("repo");
        let home_default = fixture
            .home
            .join("self/sample_project/config/provisioning/linux/repos.toml");
        touch(&home_default);

        assert_eq!(
            resolve_repos_file_from_sources(None, None, &cwd, Some(&fixture.home)).unwrap(),
            home_default
        );
    }

    #[test]
    fn resolve_repos_file_errors_without_any_config_source() {
        let fixture = ManagedFixture::new("resolve-missing");
        let cwd = fixture.root.join("elsewhere").join("repo");

        let err = resolve_repos_file_from_sources(None, None, &cwd, Some(&fixture.home))
            .expect_err("no manifest should be resolvable");
        let message = err.to_string();
        assert!(
            message.contains("--repos-file")
                && message.contains("GIT_TOOLS_MANAGED_REPOS_FILE")
                && message.contains("self/sample_project"),
            "error should point at supported manifest sources, got: {message}"
        );
    }

    #[test]
    fn parse_manifest_skips_paused_repos() {
        let home = Path::new("/home/u");
        let raw = "[[repo]]\npath='self/cfg'\nremote='git@x:c.git'\n\
                   [[repo]]\npath='work/sample_project'\nremote='git@x:i.git'\nmanaged=false\n";
        let repos = parse_manifest(raw, home).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "self/cfg");
    }
}
