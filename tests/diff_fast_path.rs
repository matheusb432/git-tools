use std::fs;
use std::path::Path;
use std::process::Command;

fn tiny_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let g = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    g(&["init", "-q"]);
    g(&["config", "user.email", "t@t"]);
    g(&["config", "user.name", "t"]);
    fs::write(repo.path().join("a.txt"), "a\n").unwrap();
    g(&["add", "."]);
    g(&["commit", "-qm", "base"]);
    g(&["branch", "-M", "main"]);
    fs::write(repo.path().join("a.txt"), "a\nb\n").unwrap();
    g(&["add", "."]);
    g(&["commit", "-qm", "change"]);
    repo
}

fn run_diff(repo: &tempfile::TempDir, store: &tempfile::TempDir, args: &[&str]) -> String {
    let stdout = assert_cmd::Command::cargo_bin("git-tools")
        .unwrap()
        .current_dir(repo.path())
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .env("GIT_TOOLS_NO_OPEN", "1")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(stdout).unwrap()
}

fn collect_json_paths(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_paths(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
}

fn sidecar_titles(store: &tempfile::TempDir) -> Vec<String> {
    let mut paths = Vec::new();
    collect_json_paths(store.path(), &mut paths);
    let mut titles = paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(path).unwrap();
            serde_json::from_str::<gtl_store::Sidecar>(&text)
                .unwrap()
                .title
        })
        .collect::<Vec<_>>();
    titles.sort();
    titles
}

// Builds a tiny repo with two commits, runs `gtl diff HEAD~1..HEAD` twice
// against a shared store, and asserts the second run reports reuse.
#[test]
fn second_identical_range_run_reuses_artifact() {
    let repo = tiny_repo();
    let store = tempfile::tempdir().unwrap();

    let _first = run_diff(&repo, &store, &["diff", "HEAD~1..HEAD"]);
    let second = run_diff(&repo, &store, &["diff", "HEAD~1..HEAD"]);
    assert!(
        second.contains("reusing"),
        "second run should reuse: {second}"
    );
}

#[test]
fn named_range_run_records_the_name_in_history_metadata() {
    let repo = tiny_repo();
    let store = tempfile::tempdir().unwrap();

    let output = run_diff(
        &repo,
        &store,
        &["diff", "--name", "end-of-day", "HEAD~1..HEAD"],
    );

    assert!(
        !output.contains("reusing"),
        "named run should render its named artifact: {output}"
    );
    assert_eq!(sidecar_titles(&store), vec!["end-of-day"]);
}

#[test]
fn differently_named_range_runs_create_distinct_history_entries() {
    let repo = tiny_repo();
    let store = tempfile::tempdir().unwrap();

    run_diff(&repo, &store, &["diff", "-n", "morning", "HEAD~1..HEAD"]);
    run_diff(&repo, &store, &["diff", "-n", "end-of-day", "HEAD~1..HEAD"]);

    assert_eq!(sidecar_titles(&store), vec!["end-of-day", "morning"]);
}
