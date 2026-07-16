//! Reading a repo's working-tree state through the [`GitRunner`] port — the
//! `git status --porcelain` fan-in shared by the `commit` and `status` flows.

use std::path::Path;

use domain::managed::working_tree::{CommitFile, DirtyState};

use crate::ports::GitRunner;

/// The repo's working-tree state: absent when its `.git` entry is missing, else
/// the parsed `git status --porcelain` file list (empty on a git failure).
pub fn dirty_state(git: &impl GitRunner, repo: &Path) -> DirtyState {
    if !git.repo_present(repo) {
        return DirtyState {
            present: false,
            dirty: false,
            files: Vec::new(),
        };
    }

    let output = match git.run(repo, &["status", "--porcelain"]) {
        Ok(output) if output.success() => output.stdout,
        _ => String::new(),
    };
    let files = parse_porcelain(&output);

    DirtyState {
        present: true,
        dirty: !files.is_empty(),
        files,
    }
}

/// Parses `git status --porcelain` output into the changed-file list.
fn parse_porcelain(output: &str) -> Vec<CommitFile> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| CommitFile {
            status: line.get(0..2).unwrap_or("").trim().to_string(),
            path: line.get(3..).unwrap_or("").to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeGitRunner;

    #[test]
    fn absent_repo_reports_not_present_without_calling_git() {
        let runner = FakeGitRunner::default();
        runner
            .absent_repos
            .lock()
            .unwrap()
            .push("/repos/gone".into());

        let state = dirty_state(&runner, Path::new("/repos/gone"));

        assert_eq!(
            state,
            DirtyState {
                present: false,
                dirty: false,
                files: Vec::new(),
            }
        );
        assert!(runner.arg_lists().is_empty(), "absent repos never call git");
    }

    #[test]
    fn porcelain_lines_parse_into_status_and_path() {
        let runner =
            FakeGitRunner::new(vec![FakeGitRunner::ok(" M src/lib.rs\n?? new-file.txt\n")]);

        let state = dirty_state(&runner, Path::new("/repos/api"));

        assert!(state.present);
        assert!(state.dirty);
        assert_eq!(
            state.files,
            vec![
                CommitFile {
                    status: "M".into(),
                    path: "src/lib.rs".into(),
                },
                CommitFile {
                    status: "??".into(),
                    path: "new-file.txt".into(),
                },
            ]
        );
    }

    #[test]
    fn clean_tree_and_git_failure_both_read_as_not_dirty() {
        let clean = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);
        assert!(!dirty_state(&clean, Path::new("/repos/api")).dirty);

        let failing = FakeGitRunner::new(vec![FakeGitRunner::exit_err("boom", 1)]);
        let state = dirty_state(&failing, Path::new("/repos/api"));
        assert!(state.present);
        assert!(!state.dirty);
    }
}
