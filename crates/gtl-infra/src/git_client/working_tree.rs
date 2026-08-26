use std::collections::BTreeMap;

use anyhow::Context as _;
use gix::bstr::ByteSlice as _;
use gtl_application::ports::{GitWorkingTree, GitWorkingTreeSummary};
use gtl_models::{
    paths::{RepositoryRelativePath, RepositoryRoot},
    repository::{PathCount, working_tree::CommitFile},
};

pub(super) fn read(repo_path: &RepositoryRoot) -> anyhow::Result<GitWorkingTree> {
    let repository = gix::open(repo_path.as_ref()).context("open Git repository")?;
    read_repository(&repository)
}

pub(super) fn read_repository(repository: &gix::Repository) -> anyhow::Result<GitWorkingTree> {
    read_repository_with_known_descendants(repository, &BTreeMap::new())
}

pub(super) fn read_repository_with_known_descendants(
    repository: &gix::Repository,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> anyhow::Result<GitWorkingTree> {
    if !known_descendants.is_empty() {
        return read_with_explicit_submodules(
            repository,
            UntrackedMode::Include,
            known_descendants,
        );
    }
    // Explicit traversal preserves worktree context lost by gix's built-in nested status.
    if has_nested_submodules(repository)? {
        read_with_explicit_submodules(repository, UntrackedMode::Include, known_descendants)
    } else {
        read_builtin(repository)
    }
}

fn has_nested_submodules(repository: &gix::Repository) -> anyhow::Result<bool> {
    let Some(submodules) = repository.submodules()? else {
        return Ok(false);
    };

    for submodule in submodules {
        let Some(submodule_repository) = submodule.open()? else {
            continue;
        };
        if submodule_repository
            .submodules()?
            .is_some_and(|mut nested| nested.next().is_some())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn read_builtin(repository: &gix::Repository) -> anyhow::Result<GitWorkingTree> {
    let changes = repository
        .status(gix::progress::Discard)
        .context("configure Git status")?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(Vec::<gix::bstr::BString>::new())
        .context("start Git status")?;
    let mut files = BTreeMap::<RepositoryRelativePath, FileState>::new();

    for change in changes {
        let change = change.context("read Git status entry")?;
        let Some(update) = StatusUpdate::from_change(&change) else {
            continue;
        };
        let path = repository_path(change.location())?;
        files.entry(path).or_default().apply(update);
    }

    Ok(into_working_tree(files))
}

fn read_with_explicit_submodules(
    repository: &gix::Repository,
    untracked: UntrackedMode,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> anyhow::Result<GitWorkingTree> {
    let changes = repository
        .status(gix::progress::Discard)
        .context("configure Git status")?
        .index_worktree_submodules(None::<gix::status::Submodule>)
        .untracked_files(untracked.gix_mode())
        .into_iter(Vec::<gix::bstr::BString>::new())
        .context("start Git status")?;
    let mut files = BTreeMap::<RepositoryRelativePath, FileState>::new();

    for change in changes {
        let change = change.context("read Git status entry")?;
        let Some(update) = StatusUpdate::from_change(&change) else {
            continue;
        };
        let path = repository_path(change.location())?;
        files.entry(path).or_default().apply(update);
    }

    merge_submodule_statuses(repository, untracked, known_descendants, &mut files)?;
    Ok(into_working_tree(files))
}

fn merge_submodule_statuses(
    repository: &gix::Repository,
    inherited_untracked: UntrackedMode,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
    files: &mut BTreeMap<RepositoryRelativePath, FileState>,
) -> anyhow::Result<()> {
    let global_ignore = global_submodule_ignore(repository)?;
    let Some(submodules) = repository.submodules()? else {
        return Ok(());
    };

    for submodule in submodules {
        let submodule_path = submodule.path()?;
        let path = repository_path(submodule_path.as_bstr())?;
        let ignore = global_ignore.or(submodule.ignore()?).unwrap_or_default();
        match ignore {
            gix::submodule::config::Ignore::All => {
                let remove = files.get_mut(&path).is_some_and(|file| {
                    file.worktree = None;
                    file.untracked = false;
                    file.index.is_none()
                });
                if remove {
                    files.remove(&path);
                }
            }
            _ if submodule_is_dirty(
                &submodule,
                ignore,
                inherited_untracked,
                known_descendants,
            )? =>
            {
                files
                    .entry(path)
                    .or_default()
                    .apply(StatusUpdate::Worktree('M'));
            }
            _ => {}
        }
    }
    Ok(())
}

fn submodule_is_dirty(
    submodule: &gix::Submodule<'_>,
    ignore: gix::submodule::config::Ignore,
    inherited_untracked: UntrackedMode,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> anyhow::Result<bool> {
    let commit_status = submodule.status(gix::submodule::config::Ignore::Dirty, true)?;
    match commit_status.is_dirty() {
        Some(true) => return Ok(true),
        None => return Ok(false),
        Some(false) => {}
    }
    if ignore == gix::submodule::config::Ignore::Dirty {
        return Ok(false);
    }
    if ignore == gix::submodule::config::Ignore::None
        && inherited_untracked == UntrackedMode::Include
        && let Some(summary) = known_submodule_summary(submodule, known_descendants)?
    {
        return Ok(summary.is_dirty());
    }

    let untracked = if inherited_untracked == UntrackedMode::Exclude
        || ignore == gix::submodule::config::Ignore::Untracked
    {
        UntrackedMode::Exclude
    } else {
        UntrackedMode::Include
    };
    let status_ignore =
        if untracked == UntrackedMode::Exclude && ignore == gix::submodule::config::Ignore::None {
            gix::submodule::config::Ignore::Untracked
        } else {
            ignore
        };
    let status = submodule.status_opts(status_ignore, true, &mut |status| {
        status.index_worktree_submodules(None::<gix::status::Submodule>)
    })?;
    match status.is_dirty() {
        Some(true) => return Ok(true),
        None => return Ok(false),
        Some(false) => {}
    }
    let Some(repository) = submodule.open()? else {
        return Ok(false);
    };
    has_dirty_submodule(&repository, untracked, known_descendants)
}

fn known_submodule_summary(
    submodule: &gix::Submodule<'_>,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> anyhow::Result<Option<GitWorkingTreeSummary>> {
    let work_dir = submodule.work_dir()?;
    let canonical_work_dir = std::fs::canonicalize(&work_dir).unwrap_or(work_dir);
    let root = RepositoryRoot::try_new(canonical_work_dir)?;
    Ok(known_descendants.get(&root).copied())
}

fn has_dirty_submodule(
    repository: &gix::Repository,
    inherited_untracked: UntrackedMode,
    known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
) -> anyhow::Result<bool> {
    let global_ignore = global_submodule_ignore(repository)?;
    let Some(submodules) = repository.submodules()? else {
        return Ok(false);
    };

    for submodule in submodules {
        let ignore = global_ignore.or(submodule.ignore()?).unwrap_or_default();
        if ignore != gix::submodule::config::Ignore::All
            && submodule_is_dirty(&submodule, ignore, inherited_untracked, known_descendants)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn global_submodule_ignore(
    repository: &gix::Repository,
) -> anyhow::Result<Option<gix::submodule::config::Ignore>> {
    let Some(value) = repository
        .config_snapshot()
        .string(gix::config::tree::Diff::IGNORE_SUBMODULES)
    else {
        return Ok(None);
    };
    gix::submodule::config::Ignore::try_from(value.as_bstr())
        .map(Some)
        .map_err(|()| anyhow::anyhow!("invalid diff.ignoreSubmodules value {value:?}"))
}

fn repository_path(path: &gix::bstr::BStr) -> anyhow::Result<RepositoryRelativePath> {
    RepositoryRelativePath::try_new(gix::path::from_bstr(path).into_owned())
        .with_context(|| format!("validate Git status path {path:?}"))
}

fn into_working_tree(files: BTreeMap<RepositoryRelativePath, FileState>) -> GitWorkingTree {
    let staged = PathCount::from_len(files.values().filter(|file| file.index.is_some()).count());
    let unprepared = PathCount::from_len(
        files
            .values()
            .filter(|file| file.untracked || file.worktree.is_some())
            .count(),
    );
    let files = files
        .into_iter()
        .map(|(path, state)| CommitFile {
            status: state.code(),
            path,
        })
        .collect();

    GitWorkingTree {
        files,
        staged,
        unprepared,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UntrackedMode {
    Include,
    Exclude,
}

impl UntrackedMode {
    const fn gix_mode(self) -> gix::status::UntrackedFiles {
        match self {
            Self::Include => gix::status::UntrackedFiles::Files,
            Self::Exclude => gix::status::UntrackedFiles::None,
        }
    }
}

#[derive(Debug, Default)]
struct FileState {
    index: Option<char>,
    worktree: Option<char>,
    untracked: bool,
}

impl FileState {
    fn apply(&mut self, update: StatusUpdate) {
        match update {
            StatusUpdate::Index(code) => self.index = Some(code),
            StatusUpdate::Worktree(code) => self.worktree = Some(code),
            StatusUpdate::Conflict => {
                self.index = Some('U');
                self.worktree = Some('U');
            }
            StatusUpdate::Untracked => self.untracked = true,
        }
    }

    fn code(&self) -> String {
        if self.untracked {
            return "??".to_owned();
        }
        self.index.into_iter().chain(self.worktree).collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum StatusUpdate {
    Index(char),
    Worktree(char),
    Conflict,
    Untracked,
}

impl StatusUpdate {
    fn from_change(change: &gix::status::Item) -> Option<Self> {
        match change {
            gix::status::Item::TreeIndex(change) => Some(Self::Index(match change {
                gix::diff::index::Change::Addition { .. } => 'A',
                gix::diff::index::Change::Deletion { .. } => 'D',
                gix::diff::index::Change::Modification {
                    previous_entry_mode,
                    entry_mode,
                    ..
                } if previous_entry_mode != entry_mode => 'T',
                gix::diff::index::Change::Modification { .. } => 'M',
                gix::diff::index::Change::Rewrite { copy: true, .. } => 'C',
                gix::diff::index::Change::Rewrite { copy: false, .. } => 'R',
            })),
            gix::status::Item::IndexWorktree(change) => {
                use gix::status::index_worktree::iter::Summary;

                match change.summary()? {
                    Summary::Added => Some(Self::Untracked),
                    Summary::Removed => Some(Self::Worktree('D')),
                    Summary::Modified => Some(Self::Worktree('M')),
                    Summary::TypeChange => Some(Self::Worktree('T')),
                    Summary::Renamed => Some(Self::Worktree('R')),
                    Summary::Copied => Some(Self::Worktree('C')),
                    Summary::IntentToAdd => Some(Self::Worktree('A')),
                    Summary::Conflict => Some(Self::Conflict),
                }
            }
        }
    }
}
