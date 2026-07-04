//! The real `RemoteSync` adapter: `tokio::process::Command`-backed git subprocess
//! calls, genuinely async so `push_all`/`pull_all` get real concurrent I/O without
//! a thread pool.

use std::path::Path;

use anyhow::Context;
use application::ports::{RemoteSync, SyncOutput};
use tokio::process::Command;

#[derive(Debug, Default, Clone, Copy)]
pub struct TokioRemoteSync;

/// One captured git invocation's raw parts, before the port-facing methods decide
/// what to keep (some need `stdout` alone, others need `success` + combined text).
struct Capture {
    success: bool,
    stdout: String,
    stderr: String,
}

impl Capture {
    fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

async fn capture(repo: &Path, args: &[&str]) -> anyhow::Result<Capture> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .with_context(|| format!("failed to run git in {}", repo.display()))?;
    Ok(Capture {
        success: output.status.success(),
        stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
        stderr: String::from_utf8(output.stderr).context("git stderr was not valid UTF-8")?,
    })
}

impl RemoteSync for TokioRemoteSync {
    fn repo_present(&self, repo: &Path) -> bool {
        repo.join(".git").exists()
    }

    async fn current_branch(&self, repo: &Path) -> anyhow::Result<String> {
        let out = capture(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        anyhow::ensure!(out.success, "git rev-parse --abbrev-ref HEAD failed");
        Ok(out.stdout.trim().to_string())
    }

    async fn has_remote(&self, repo: &Path, remote: &str) -> anyhow::Result<bool> {
        let out = capture(repo, &["remote", "get-url", remote]).await?;
        Ok(out.success)
    }

    async fn push(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        dry: bool,
    ) -> anyhow::Result<SyncOutput> {
        let mut args = vec!["push", remote, branch];
        if dry {
            args.push("--dry-run");
        }
        let out = capture(repo, &args).await?;
        Ok(SyncOutput {
            success: out.success,
            combined: out.combined(),
        })
    }

    async fn fetch(&self, repo: &Path, remote: &str) -> anyhow::Result<SyncOutput> {
        let out = capture(repo, &["fetch", remote]).await?;
        Ok(SyncOutput {
            success: out.success,
            combined: out.combined(),
        })
    }

    async fn upstream_ref(&self, repo: &Path) -> anyhow::Result<Option<String>> {
        let out = capture(
            repo,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )
        .await?;
        if !out.success || out.stdout.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(out.stdout.trim().to_string()))
    }

    async fn verify_ref(&self, repo: &Path, remote_ref: &str) -> anyhow::Result<bool> {
        let out = capture(repo, &["rev-parse", "--verify", "--quiet", remote_ref]).await?;
        Ok(out.success)
    }

    async fn rev_list_count(&self, repo: &Path, range: &str) -> anyhow::Result<usize> {
        let out = capture(repo, &["rev-list", "--count", range]).await?;
        anyhow::ensure!(out.success, "git rev-list --count failed");
        Ok(out.stdout.trim().parse().unwrap_or(0))
    }

    async fn rev_list_left_right(
        &self,
        repo: &Path,
        range: &str,
    ) -> anyhow::Result<(usize, usize)> {
        let out = capture(repo, &["rev-list", "--count", "--left-right", range]).await?;
        anyhow::ensure!(out.success, "git rev-list --count --left-right failed");
        let mut parts = out.stdout.split_whitespace();
        let behind = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let ahead = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        Ok((behind, ahead))
    }

    async fn merge_ff_only(&self, repo: &Path, remote_ref: &str) -> anyhow::Result<SyncOutput> {
        let out = capture(repo, &["merge", "--ff-only", remote_ref]).await?;
        Ok(SyncOutput {
            success: out.success,
            combined: out.combined(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use application::ports::RemoteSync;
    use tempfile::TempDir;

    use super::*;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    fn git_out(dir: &std::path::Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// A bare origin plus a clone with an initial commit, both under one `TempDir`.
    fn fixture() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let origin = dir.path().join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", "main"]);

        let seed = dir.path().join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        git(&seed, &["init", "-b", "main"]);
        git(&seed, &["config", "user.email", "t@example.com"]);
        git(&seed, &["config", "user.name", "t"]);
        std::fs::write(seed.join("a.txt"), "one\n").unwrap();
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-m", "seed"]);
        git(
            &seed,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&seed, &["push", "origin", "main"]);

        let local = dir.path().join("local");
        Command::new("git")
            .args(["clone", origin.to_str().unwrap(), local.to_str().unwrap()])
            .status()
            .unwrap();
        git(&local, &["config", "user.email", "t@example.com"]);
        git(&local, &["config", "user.name", "t"]);

        (dir, origin, local)
    }

    #[tokio::test]
    async fn repo_present_checks_the_dot_git_directory() {
        let (_dir, _origin, local) = fixture();
        assert!(TokioRemoteSync.repo_present(&local));
        assert!(!TokioRemoteSync.repo_present(local.parent().unwrap()));
    }

    #[tokio::test]
    async fn current_branch_and_has_remote_reflect_a_real_clone() {
        let (_dir, _origin, local) = fixture();
        let branch = TokioRemoteSync.current_branch(&local).await.unwrap();
        assert_eq!(branch, "main");
        assert!(TokioRemoteSync.has_remote(&local, "origin").await.unwrap());
        assert!(!TokioRemoteSync.has_remote(&local, "nope").await.unwrap());
    }

    #[tokio::test]
    async fn push_advances_the_remote_and_upstream_ref_reports_it() {
        let (_dir, origin, local) = fixture();
        git(&local, &["push", "-u", "origin", "main"]);
        std::fs::write(local.join("b.txt"), "two\n").unwrap();
        git(&local, &["add", "-A"]);
        git(&local, &["commit", "-m", "local change"]);
        let local_head = git_out(&local, &["rev-parse", "HEAD"]);

        let upstream = TokioRemoteSync.upstream_ref(&local).await.unwrap();
        assert_eq!(upstream.as_deref(), Some("origin/main"));
        let ahead = TokioRemoteSync
            .rev_list_count(&local, "@{u}..HEAD")
            .await
            .unwrap();
        assert_eq!(ahead, 1);

        let outcome = TokioRemoteSync
            .push(&local, "origin", "main", false)
            .await
            .unwrap();
        assert!(outcome.success);
        assert_eq!(
            git_out(&origin, &["rev-parse", "refs/heads/main"]),
            local_head
        );
    }

    #[tokio::test]
    async fn fetch_and_left_right_counts_reflect_a_diverged_history() {
        let (_dir, origin, local) = fixture();
        git(&local, &["push", "-u", "origin", "main"]);

        std::fs::write(local.join("local.txt"), "local\n").unwrap();
        git(&local, &["add", "-A"]);
        git(&local, &["commit", "-m", "local change"]);

        let other = TempDir::new().unwrap();
        let other_clone = other.path().join("other");
        Command::new("git")
            .args([
                "clone",
                origin.to_str().unwrap(),
                other_clone.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        git(&other_clone, &["config", "user.email", "t@example.com"]);
        git(&other_clone, &["config", "user.name", "t"]);
        std::fs::write(other_clone.join("remote.txt"), "remote\n").unwrap();
        git(&other_clone, &["add", "-A"]);
        git(&other_clone, &["commit", "-m", "remote change"]);
        git(&other_clone, &["push", "origin", "main"]);

        let fetch = TokioRemoteSync.fetch(&local, "origin").await.unwrap();
        assert!(fetch.success);
        let (behind, ahead) = TokioRemoteSync
            .rev_list_left_right(&local, "origin/main...main")
            .await
            .unwrap();
        assert!(
            behind > 0 && ahead > 0,
            "expected genuine two-sided divergence, got (behind={behind}, ahead={ahead})"
        );

        assert!(
            TokioRemoteSync
                .verify_ref(&local, "refs/remotes/origin/main")
                .await
                .unwrap()
        );
        let local_head_before_merge = git_out(&local, &["rev-parse", "HEAD"]);
        let merge = TokioRemoteSync
            .merge_ff_only(&local, "origin/main")
            .await
            .unwrap();
        assert!(
            !merge.success,
            "ff-only merge must refuse genuinely diverged history"
        );
        assert_eq!(
            git_out(&local, &["rev-parse", "HEAD"]),
            local_head_before_merge,
            "a refused ff-only merge must leave local HEAD untouched"
        );
        assert_ne!(
            git_out(&local, &["rev-parse", "HEAD"]),
            git_out(&origin, &["rev-parse", "main"])
        );
    }

    #[tokio::test]
    async fn merge_ff_only_succeeds_when_purely_behind() {
        let (_dir, origin, local) = fixture();
        git(&local, &["push", "-u", "origin", "main"]);

        // `local` makes no commits of its own; only `origin` advances (via a second
        // clone), so `local` is purely behind, not diverged.
        let other = TempDir::new().unwrap();
        let other_clone = other.path().join("other");
        Command::new("git")
            .args([
                "clone",
                origin.to_str().unwrap(),
                other_clone.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        git(&other_clone, &["config", "user.email", "t@example.com"]);
        git(&other_clone, &["config", "user.name", "t"]);
        std::fs::write(other_clone.join("remote.txt"), "remote\n").unwrap();
        git(&other_clone, &["add", "-A"]);
        git(&other_clone, &["commit", "-m", "remote change"]);
        git(&other_clone, &["push", "origin", "main"]);

        let fetch = TokioRemoteSync.fetch(&local, "origin").await.unwrap();
        assert!(fetch.success);

        let merge = TokioRemoteSync
            .merge_ff_only(&local, "origin/main")
            .await
            .unwrap();
        assert!(
            merge.success,
            "ff-only merge must succeed when purely behind: {}",
            merge.combined
        );
        assert_eq!(
            git_out(&local, &["rev-parse", "HEAD"]),
            git_out(&origin, &["rev-parse", "main"])
        );
    }
}
