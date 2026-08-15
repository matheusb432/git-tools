//! Computes the standalone patch introduced by one commit already present in a viewer range.

use std::path::{Path, PathBuf};

use gtl_models::diffs::{AppliedExclusions, Commit, CommitIdAbbreviation};

use crate::{
    diffs::{
        Cmd, Foot, View,
        assemble::{DiffData, assemble},
        view::sort_files_tree_order,
    },
    ports::{GitClient, UserSettingsLoadError, UserSettingsStore},
    shared::repository_name::from_path,
};

const EMPTY_TREE_ID: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const EMPTY_TREE_ABBREVIATED_ID: &str = "4b825dc642";

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeCommitPatch {
    pub repo_root: PathBuf,
    pub commit: Commit,
}

#[derive(Debug, thiserror::Error)]
pub enum ComputeCommitPatchError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    request: ComputeCommitPatch,
    settings: &impl UserSettingsStore,
    source: &impl GitClient,
) -> Result<View, ComputeCommitPatchError> {
    let repo_path = request.repo_root;
    let top = repo_path.to_string_lossy();
    let repo_name = from_path(top.as_ref());
    let settings = settings.load()?;
    let excluded = settings
        .diff_exclusions()
        .for_project_or_default(&repo_name);
    let commit = request.commit;
    let abbreviation = CommitIdAbbreviation::TenCharacters;
    let (base, base_abbreviated) = commit.parents.first().map_or_else(
        || (EMPTY_TREE_ID, EMPTY_TREE_ABBREVIATED_ID.to_owned()),
        |parent| (parent.as_ref(), parent.abbreviated(abbreviation)),
    );
    let diff_range = format!("{base}..{}", commit.id);
    let log_range = format!("{}^!", commit.id);
    let DiffData {
        commits,
        mut files,
        hidden_paths,
    } = assemble(
        source,
        Path::new(&repo_path),
        &diff_range,
        &log_range,
        excluded,
    )?;
    sort_files_tree_order(&mut files);

    let abbreviated_id = commit.id.abbreviated(abbreviation);
    Ok(View {
        repo_name,
        repo_root: top.into_owned(),
        branch: source.current_branch(Path::new(&repo_path))?,
        upstream: base_abbreviated.clone(),
        title: format!("commit {abbreviated_id}"),
        cmd: Cmd {
            lead: "git diff ".into(),
            range: format!("{base_abbreviated}..{abbreviated_id}"),
            trail: String::new(),
        },
        commits_label: "# selected commit".into(),
        foot: Foot {
            cmd: format!("git show --format=fuller {}", commit.id),
        },
        commits,
        files,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        FakeGitClient, FixedUserSettingsStore,
        diffs::{DIFF_SINGLE_FILE, commit_with},
    };

    const COMMIT_ID: &str = "1111111111222222222233333333334444444444";
    const PARENT_ID: &str = "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd";

    #[test]
    fn computes_a_standalone_first_parent_patch() {
        let commit = commit_with(COMMIT_ID, "selected", &[PARENT_ID]);
        let source = FakeGitClient {
            branch: "feature".into(),
            commits: vec![commit.clone()],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let view = execute(
            ComputeCommitPatch {
                repo_root: "/repo".into(),
                commit,
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("commit patch computes");

        assert_eq!(view.title, "commit 1111111111");
        assert_eq!(view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(view.commits, source.commits);
        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].added, 2);
        assert_eq!(view.files[0].removed, 1);
    }

    #[test]
    fn root_commit_uses_the_empty_tree_as_its_parent() {
        let commit = commit_with(COMMIT_ID, "root", &[]);
        let source = FakeGitClient {
            branch: "main".into(),
            commits: vec![commit.clone()],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };

        let view = execute(
            ComputeCommitPatch {
                repo_root: "/repo".into(),
                commit,
            },
            &FixedUserSettingsStore::default(),
            &source,
        )
        .expect("root patch computes");

        assert_eq!(view.upstream, "4b825dc642");
        assert_eq!(view.cmd.range, "4b825dc642..1111111111");
    }
}
