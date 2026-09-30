//! Computes the standalone patch introduced by one commit already present in a viewer range.

use gtl_models::{
    diffs::{AppliedExtensionFilter, Commit, CommitIdAbbreviation, DiffViewTitle},
    git::{GitDiffSpec, GitRange, GitRevision},
    paths::RepositoryRoot,
};

use super::{EMPTY_TREE_ABBREVIATED_ID, EMPTY_TREE_ID};
use crate::{
    diffs::{
        Cmd, FetchFullContextDiff, Foot, FullContextDiffState, View,
        assemble::{DiffData, assemble},
        fetch_full_context_diff,
        view::sort_files_tree_order,
    },
    ports::{ExtensionFilterReader, GitClient, UserSettingsLoadError, UserSettingsReader},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ComputeCommitPatch {
    pub repo_root: RepositoryRoot,
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
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    filters: &impl ExtensionFilterReader,
) -> Result<View, ComputeCommitPatchError> {
    let repo_path = request.repo_root;
    let repo_name = repo_path.project_name();
    let settings = settings.load()?;
    let filter = filters.extension_filter(&repo_path)?;
    let commit = request.commit;
    let abbreviation = CommitIdAbbreviation::TenCharacters;
    let (base, base_abbreviated) = match commit.parents.first() {
        Some(parent) => (
            GitRevision::from(parent),
            GitRevision::abbreviated_commit(parent, abbreviation),
        ),
        None => (
            GitRevision::try_new(EMPTY_TREE_ID).map_err(anyhow::Error::from)?,
            GitRevision::try_new(EMPTY_TREE_ABBREVIATED_ID).map_err(anyhow::Error::from)?,
        ),
    };
    let diff_range = GitRange::two_dot(&base, &GitRevision::from(&commit.id));
    let diff_spec = GitDiffSpec::Range(diff_range);
    let log_range = GitRange::single_commit(&commit.id);
    let DiffData {
        spec: _,
        commits,
        mut files,
        hidden_paths,
        full_context,
    } = assemble(git, &repo_path, &diff_spec, Some(&log_range), &filter, None)?;
    sort_files_tree_order(&mut files);

    let abbreviated_id = commit.id.abbreviated(abbreviation);
    let view = View {
        file_filter: crate::diffs::file_filter::DiffFileFilter::new(diff_spec, filter.clone()),
        repo_name,
        repo_root: repo_path.clone(),
        branch: git.current_branch(&repo_path)?,
        upstream: base_abbreviated.clone(),
        title: DiffViewTitle::Commit {
            id: commit.id.clone(),
        },
        cmd: Cmd {
            lead: "git diff ".into(),
            range: format!("{base_abbreviated}..{abbreviated_id}"),
            trail: String::new(),
        },
        foot: Foot {
            cmd: format!("git show --format=fuller {}", commit.id),
        },
        commits,
        files,
        full_context,
        extension_filter: AppliedExtensionFilter::from_hidden(&filter, hidden_paths),
    };
    if settings.viewer_render_options().density() == gtl_models::viewer::DiffDensity::Full
        && let FullContextDiffState::Deferred(source) = &view.full_context
    {
        let request = FetchFullContextDiff::new(&view.repo_root, source);
        let full_context =
            fetch_full_context_diff::execute(&request, git).map_err(anyhow::Error::from)?;
        return view
            .with_full_context(full_context)
            .map_err(anyhow::Error::from)
            .map_err(ComputeCommitPatchError::Unexpected);
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diffs::compute_commit_patch,
        utils::{
            FakeGitClient, FixedUserSettingsStore, SavedExtensionFilters,
            diffs::{DIFF_SINGLE_FILE, commit_with},
        },
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

        let view = compute_commit_patch::execute(
            ComputeCommitPatch {
                repo_root: crate::utils::repository_root("//fixture.invalid/repositories/repo"),
                commit: commit.clone(),
            },
            &FixedUserSettingsStore::default(),
            &source,
            &SavedExtensionFilters::default(),
        )
        .unwrap();

        assert_eq!(view.title, DiffViewTitle::Commit { id: commit.id });
        assert_eq!(view.cmd.range, "aaaaaaaaaa..1111111111");
        assert_eq!(view.commits, source.commits);
        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].added.value(), 2);
        assert_eq!(view.files[0].removed.value(), 1);
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

        let view = compute_commit_patch::execute(
            ComputeCommitPatch {
                repo_root: crate::utils::repository_root("//fixture.invalid/repositories/repo"),
                commit,
            },
            &FixedUserSettingsStore::default(),
            &source,
            &SavedExtensionFilters::default(),
        )
        .unwrap();

        assert_eq!(view.upstream.as_ref(), "4b825dc642");
        assert_eq!(view.cmd.range, "4b825dc642..1111111111");
    }
}
