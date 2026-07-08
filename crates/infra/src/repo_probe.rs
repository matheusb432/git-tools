//! `GitRepoProbe`: the [`RepoProbe`] adapter — a filesystem existence check
//! plus `git rev-parse --show-toplevel` (via the shared `git_capture` seam).

use std::path::{Path, PathBuf};

use application::ports::{RepoProbe, RepoProbeResult};

/// Probes a directory with the local `git` binary.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitRepoProbe;

impl RepoProbe for GitRepoProbe {
    fn probe(&self, dir: &Path) -> anyhow::Result<RepoProbeResult> {
        if !dir.is_dir() {
            return Ok(RepoProbeResult::NotFound);
        }
        match crate::git_capture::run_git(dir, &["rev-parse", "--show-toplevel"]) {
            Ok(out) => {
                let top = PathBuf::from(out.trim());
                let top_level = std::fs::canonicalize(&top).unwrap_or(top);
                Ok(RepoProbeResult::Repo { top_level })
            }
            Err(_) => Ok(RepoProbeResult::NotAGitRepo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dir_is_not_found() {
        let result = GitRepoProbe
            .probe(Path::new("/definitely/not/here"))
            .unwrap();
        assert_eq!(result, RepoProbeResult::NotFound);
    }

    #[test]
    fn plain_dir_is_not_a_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let result = GitRepoProbe.probe(tmp.path()).unwrap();
        assert_eq!(result, RepoProbeResult::NotAGitRepo);
    }

    #[test]
    fn a_real_repo_reports_its_top_level() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );
        let RepoProbeResult::Repo { top_level } = GitRepoProbe.probe(tmp.path()).unwrap() else {
            panic!("expected a repo");
        };
        assert_eq!(top_level, std::fs::canonicalize(tmp.path()).unwrap());
    }
}
