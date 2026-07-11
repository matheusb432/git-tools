//! Black-box wiring tests for `diff` routing behaviors that only emerge at the
//! `run(argv) -> ExitCode` / built-binary level. The decision seams themselves
//! (`viewer::has_display`, recipe construction, and `commands::diff::render`) are already
//! unit-tested inline with fakes; this file drives the real binary end-to-end
//! to prove those seams are actually wired together correctly: headless degradation lands
//! a real artifact in the store, the default path renders the artifact even under
//! `GIT_TOOLS_NO_OPEN` (opening nothing), `diff live` persistence, and an empty
//! `diff --all` doesn't hard-error. Follows the `viewer_headless.rs`/`e2e.rs` idiom: a real
//! temp repo, an isolated `GIT_TOOLS_DATA_DIR`/`GIT_TOOLS_CONFIG`, and `env_remove` for
//! `DISPLAY`/`WAYLAND_DISPLAY` to force headless.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as Git,
};

use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt, str::contains};
use tempfile::TempDir;

mod common;

/// A repo with one commit and an isolated central store; tests add upstream/commits/
/// manifests as needed.
struct Repo {
    _tmp: TempDir,
    _store: TempDir,
    root: PathBuf,
    dir: PathBuf,
    store_dir: PathBuf,
}

impl Repo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let store_dir = store.path().to_path_buf();
        let root = tmp.path().to_path_buf();
        let dir = root.join("repo");
        fs::create_dir_all(&dir).unwrap();
        let this = Self {
            _tmp: tmp,
            _store: store,
            root,
            dir,
            store_dir,
        };
        this.git(&["init", "-q", "-b", "main"]);
        this.git(&["config", "user.email", "routing@example.invalid"]);
        this.git(&["config", "user.name", "Routing Bot"]);
        this.git(&["config", "commit.gpgsign", "false"]);
        this.commit("a.txt", "base\n", "chore: base");
        this
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Git::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn commit(&self, file: &str, contents: &str, message: &str) {
        fs::write(self.dir.join(file), contents).unwrap();
        self.git(&["add", "-A"]);
        let out = Git::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["commit", "-m", message])
            .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00")
            .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Adds a bare `origin` and pushes `main`, establishing an upstream (`@{u}`).
    fn add_upstream(&self) {
        let remote = self.root.join("origin.git");
        assert!(
            Git::new("git")
                .args(["init", "--bare", "-q"])
                .arg(&remote)
                .status()
                .unwrap()
                .success()
        );
        self.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        self.git(&["push", "-q", "-u", "origin", "HEAD"]);
    }

    /// A `git-tools` invocation rooted in the repo, with an isolated store and no real
    /// user config read.
    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("git-tools").unwrap();
        cmd.args(args)
            .current_dir(&self.dir)
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            .env("GIT_TOOLS_DAEMON_IDLE_SECS", "2")
            .env("GIT_TOOLS_CONFIG", "/dev/null");
        cmd
    }

    /// Every file under the store's `diffs/` tree (empty unless the raw/store path
    /// actually rendered something).
    fn diffs_written(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        collect_files(&self.store_dir.join("diffs"), &mut out);
        out
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Extract the artifact path from a "wrote <path>" stdout line.
fn artifact_from_stdout(stdout: &str) -> Option<PathBuf> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("wrote ").map(|p| PathBuf::from(p.trim())))
}

// --- 1. headless degradation -------------------------------------------------------

/// Default `diff` with no display must degrade to the raw/store path rather than
/// forwarding to a viewer: an artifact is actually written under the central store.
/// `viewer_headless.rs` already pins the stdout text + repo cleanliness; this adds the
/// missing piece — resolving the printed path and asserting the file genuinely exists.
#[test]
fn headless_default_diff_degrades_to_the_store_and_writes_a_real_artifact() {
    let repo = Repo::new();
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    common::ensure_daemon_built();

    let output = repo
        .cmd(&["diff"])
        .env("GIT_TOOLS_NO_OPEN", "1") // hermetic: never spawn a real browser
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let artifact = artifact_from_stdout(&stdout)
        .unwrap_or_else(|| panic!("expected a 'wrote <path>' line, got: {stdout}"));
    assert!(
        artifact.starts_with(&repo.store_dir),
        "artifact must land in the central store, not a viewer forward: {}",
        artifact.display()
    );
    assert!(
        artifact.exists(),
        "artifact must actually exist on disk at {}",
        artifact.display()
    );
}

// --- 2. GIT_TOOLS_NO_OPEN renders but opens nothing -------------------------------

/// `GIT_TOOLS_NO_OPEN=1` preserves the current daemon/store behavior even when a display
/// is present, but opens nothing: no viewer spawn, no browser, and no viewer-unavailable
/// diagnostic.
#[test]
fn no_open_renders_the_artifact_but_opens_nothing() {
    let repo = Repo::new();
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    common::ensure_daemon_built();

    repo.cmd(&["diff"])
        .env("GIT_TOOLS_NO_OPEN", "1")
        // has_display() only checks presence, not a live connection - safe to fake so
        // this test exercises the viewer-handoff branch instead of the headless path.
        .env("DISPLAY", ":99")
        .assert()
        .success()
        .stdout(contains("wrote"))
        .stderr(contains("viewer unavailable").not());

    assert!(
        !repo.diffs_written().is_empty(),
        "the default path must render the artifact to the store even under NO_OPEN"
    );
}

/// Sanity check that the fix didn't disturb the regular `diff` daemon-error path: a
/// genuine handler error rides as a `NoteLevel::Error` note (via the daemon's generic
/// error wrapper). `error_text` still prefers the `Error` note over any `Warn`, and
/// `print_wire_notes` skips `Error` notes, so the message prints exactly once with no
/// spurious generic fallback — unchanged by the live-view fix.
#[test]
fn diff_raw_daemon_error_prints_its_message_once() {
    let repo = Repo::new();
    common::ensure_daemon_built();

    let output = repo
        .cmd(&["diff", "--raw", "deadbeef"])
        .env("GIT_TOOLS_NO_OPEN", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "must exit 1 on a bad commit");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.matches("not a commit: deadbeef").count(),
        1,
        "the daemon error message must appear exactly once, got stderr: {stderr}"
    );
    assert!(
        !stderr.contains("daemon reported an error"),
        "no generic fallback line, got stderr: {stderr}"
    );
}

// --- 3. `diff live` persists before forwarding -----------------------------------

#[test]
fn diff_live_with_no_open_saves_but_forwards_nothing() {
    let repo = Repo::new();
    common::ensure_daemon_built();

    repo.cmd(&["diff", "live", "--path", repo.dir.to_str().unwrap()])
        .env("GIT_TOOLS_NO_OPEN", "1")
        .assert()
        .success()
        .stdout(contains("saved live view"));
}

#[test]
fn diff_live_bad_path_prints_the_daemons_rejection_message_once() {
    let repo = Repo::new();
    common::ensure_daemon_built();
    let not_a_repo = tempfile::tempdir().unwrap();

    let output = repo
        .cmd(&[
            "diff",
            "live",
            "--path",
            not_a_repo.path().to_str().unwrap(),
        ])
        .env("GIT_TOOLS_NO_OPEN", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let message = format!(
        "The directory `{}` is not a git repository.",
        not_a_repo.path().display()
    );
    assert_eq!(stderr.matches(&message).count(), 1, "stderr: {stderr}");
    assert!(
        !stderr.contains("daemon reported an error"),
        "stderr: {stderr}"
    );
}

// --- 4. empty `diff --all` --------------------------------------------------------

/// A managed repo with nothing unpushed must not hard-error `diff --all`. Current
/// (pinned) behavior: the empty, already-filtered repo list still reaches the
/// `render_diff_all` slice, which renders a zero-tab artifact rather than skipping —
/// exit 0 with a clear `0 repo(s)` note, not a failure.
#[test]
fn diff_all_with_nothing_unpushed_does_not_hard_error() {
    let repo = Repo::new();
    repo.add_upstream(); // clean and up to date: nothing unpushed
    common::ensure_daemon_built();
    let manifest = repo.root.join("repos.toml");
    fs::write(
        &manifest,
        "[[repo]]\npath = \"repo\"\nremote = \"origin\"\n",
    )
    .unwrap();

    repo.cmd(&[
        "diff",
        "--all",
        "--repos-file",
        manifest.to_str().unwrap(),
        "--home-dir",
        repo.root.to_str().unwrap(),
    ])
    .env("GIT_TOOLS_NO_OPEN", "1")
    .env_remove("DISPLAY")
    .env_remove("WAYLAND_DISPLAY")
    .assert()
    .success()
    .stdout(contains("diff-all: 0 repo(s)"))
    .stdout(contains("wrote"));
}
