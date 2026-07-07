//! Shared test fixtures for the `managed` submodules: a throwaway `$HOME` plus real git
//! repos/remotes, so push/pull/commit/status/manifest tests exercise real `git` behavior.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct ManagedFixture {
    pub(super) root: PathBuf,
    pub(super) home: PathBuf,
    pub(super) manifest: PathBuf,
}

impl ManagedFixture {
    pub(super) fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("git-tools-{name}-{unique}"));
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let manifest = root.join("repos.toml");
        Self {
            root,
            home,
            manifest,
        }
    }

    pub(super) fn init_repo(&self, name: &str) -> PathBuf {
        let repo = self.home.join(name);
        std::fs::create_dir_all(&repo).unwrap();
        cmd("git", &["init", "-b", "main", repo.to_str().unwrap()]);
        configure_repo(&repo);
        self.write_file(&format!("{name}/README.md"), "base\n");
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "base"]);
        repo
    }

    pub(super) fn write_manifest(&self, entries: &[(&str, &str)]) {
        let text = entries
            .iter()
            .fold(String::new(), |mut out, (path, remote)| {
                use std::fmt::Write as _;
                let _ = write!(
                    out,
                    "[[repo]]\npath = \"{path}\"\nremote = \"{remote}\"\n\n"
                );
                out
            });
        std::fs::write(&self.manifest, text).unwrap();
    }

    pub(super) fn write_file(&self, relative: &str, text: &str) {
        let path = self.home.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }
}

impl Drop for ManagedFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn configure_repo(repo: &Path) {
    git(repo, &["config", "user.name", "Test User"]);
    git(repo, &["config", "user.email", "test@example.invalid"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
}

pub(super) fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn git_out(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

pub(super) fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, "").unwrap();
}

fn cmd(program: &str, args: &[&str]) {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{} {} failed\nstdout: {}\nstderr: {}",
        program,
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
