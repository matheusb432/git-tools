use std::path::{Path, PathBuf};

use crate::cli::DiffTarget;
use crate::git;

pub fn run(
    repo: impl AsRef<Path>,
    monorepo: impl AsRef<Path>,
    base: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let top = git::top_level(repo)?;
    let target = match base.map(str::trim).filter(|base| !base.is_empty()) {
        Some(base) => DiffTarget::Base(base.to_string()),
        None => DiffTarget::Unpushed,
    };
    super::diff::render(&top, monorepo.as_ref(), &target)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempRepo {
        root: PathBuf,
        repo: PathBuf,
        monorepo: PathBuf,
    }

    impl TempRepo {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("git-tools-{name}-{unique}"));
            let repo = root.join("subrepo");
            let monorepo = root.join("outer");
            std::fs::create_dir_all(&repo).unwrap();
            std::fs::create_dir_all(&monorepo).unwrap();
            cmd("git", &["init", "-b", "main", repo.to_str().unwrap()]);
            git(&repo, &["config", "user.name", "Test User"]);
            git(&repo, &["config", "user.email", "test@example.invalid"]);
            git(&repo, &["config", "commit.gpgsign", "false"]);
            std::fs::write(repo.join("README.md"), "base\n").unwrap();
            git(&repo, &["add", "-A"]);
            git(&repo, &["commit", "-m", "base"]);
            Self {
                root,
                repo,
                monorepo,
            }
        }

        fn head(&self) -> String {
            git_out(&self.repo, &["rev-parse", "HEAD"])
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn cmd(program: &str, args: &[&str]) {
        let output = Command::new(program).args(args).output().unwrap();
        assert!(output.status.success(), "{program} {args:?} failed");
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_out(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn writes_artifact_under_monorepo_not_subrepo() {
        let fixture = TempRepo::new("subrepos-base");
        // working-tree change so the diff is non-empty
        std::fs::write(fixture.repo.join("README.md"), "changed\n").unwrap();
        let base = fixture.head();

        let out = super::run(&fixture.repo, &fixture.monorepo, Some(&base)).unwrap();

        assert!(out.exists(), "artifact should exist at {}", out.display());
        assert!(
            out.starts_with(&fixture.monorepo),
            "artifact {} should live under monorepo {}",
            out.display(),
            fixture.monorepo.display()
        );
        assert!(
            !out.starts_with(&fixture.repo),
            "artifact must not land in the subrepo"
        );
    }
}
