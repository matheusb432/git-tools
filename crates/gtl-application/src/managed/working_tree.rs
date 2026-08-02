//! Reading a repo's working-tree state through the [`GitClient`] port — the
//! `git status --porcelain` fan-in shared by the `commit` and `status` flows.

use std::path::Path;

use gtl_models::managed::working_tree::DirtyState;

use crate::ports::GitClient;

/// The repo's working-tree state: absent when its `.git` entry is missing, else
/// the parsed `git status --porcelain` file list (empty on a git failure).
pub fn dirty_state(git: &impl GitClient, repo: &Path) -> DirtyState {
    dirty_state_checked(git, repo).unwrap_or(DirtyState {
        present: true,
        dirty: false,
        files: Vec::new(),
    })
}

pub(super) fn dirty_state_checked(git: &impl GitClient, repo: &Path) -> anyhow::Result<DirtyState> {
    if !git.repo_present(repo) {
        return Ok(DirtyState {
            present: false,
            dirty: false,
            files: Vec::new(),
        });
    }

    let files = match git.working_tree(repo)? {
        crate::ports::GitEffect::Applied(tree) => tree.files,
        crate::ports::GitEffect::Rejected(_) => Vec::new(),
    };

    Ok(DirtyState {
        present: true,
        dirty: !files.is_empty(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::managed::working_tree::CommitFile;

    use super::*;
    use crate::testing::ScriptedGitClient;

    #[test]
    fn absent_repo_reports_not_present() {
        let runner = ScriptedGitClient::default();
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
    }

    #[test]
    fn porcelain_lines_parse_into_status_and_path() {
        let runner = ScriptedGitClient::new(vec![ScriptedGitClient::applied(
            " M src/lib.rs\n?? new-file.txt\n",
        )]);

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
        let clean = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);
        assert!(!dirty_state(&clean, Path::new("/repos/api")).dirty);

        let failing = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("boom")]);
        let state = dirty_state(&failing, Path::new("/repos/api"));
        assert!(state.present);
        assert!(!state.dirty);
    }
}
