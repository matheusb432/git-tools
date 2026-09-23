use std::{
    collections::BTreeSet,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context as _;
use gix::bstr::ByteSlice as _;

use super::PLAN_BYTES_MAX;
const WATCHES_PER_PROJECT_MAX: usize = 512;
const PLAN_ENTRIES_MAX: usize = 100_000;

#[derive(Default)]
pub(super) struct Plan {
    pub(super) directories: BTreeSet<PathBuf>,
    worktrees: Vec<PathBuf>,
    ignored: BTreeSet<PathBuf>,
    metadata: BTreeSet<PathBuf>,
    files: BTreeSet<PathBuf>,
}

impl Plan {
    pub(super) fn bytes(&self) -> usize {
        self.directories
            .iter()
            .chain(&self.ignored)
            .chain(&self.metadata)
            .chain(&self.files)
            .chain(&self.worktrees)
            .map(|path| path.as_os_str().len() + 96)
            .sum()
    }

    pub(super) fn accepts(&self, path: &Path) -> bool {
        if self.files.contains(path) {
            return true;
        }
        if self.metadata.iter().any(|root| path.starts_with(root)) {
            return path.extension().is_none_or(|extension| extension != "lock");
        }
        self.worktrees.iter().any(|root| path.starts_with(root))
            && !self.ignored.iter().any(|ignored| path.starts_with(ignored))
    }

    pub(super) fn replans(&self, path: &Path) -> bool {
        self.files.contains(path)
            || self.metadata.iter().any(|root| path.starts_with(root))
            || path.file_name().is_some_and(|name| {
                name == ".gitignore" || name == ".gitattributes" || name == ".gitmodules"
            })
    }

    fn directory(&mut self, path: PathBuf) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.directories.len() < WATCHES_PER_PROJECT_MAX,
            "project watch budget exceeded"
        );
        self.directories.insert(path);
        Ok(())
    }

    fn file(&mut self, path: PathBuf) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.files.len() < 128,
            "metadata file watch budget exceeded"
        );
        if let Some(parent) = path.parent()
            && parent.is_dir()
        {
            self.directory(parent.to_path_buf())?;
        }
        self.files.insert(path);
        Ok(())
    }
}

pub(super) fn plan(path: &Path, cancellation: &AtomicBool) -> anyhow::Result<Plan> {
    let path = match path.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let parent = path
                .parent()
                .context("missing watch root has no parent")?
                .canonicalize()?;
            parent.join(path.file_name().context("missing watch root has no name")?)
        }
        Err(error) => return Err(error.into()),
    };
    let mut plan = Plan::default();
    let mut visited = 0;
    if path.join(".git").exists() {
        collect_repository(&gix::open(&path)?, &mut plan, cancellation, &mut visited, 0)?;
    } else {
        plan.file(path.clone())?;
        if path.is_dir() {
            plan.directory(path.clone())?;
            plan.files.insert(path.join(".git"));
        }
    }
    Ok(plan)
}

fn collect_repository(
    repository: &gix::Repository,
    plan: &mut Plan,
    cancellation: &AtomicBool,
    visited: &mut usize,
    depth: usize,
) -> anyhow::Result<()> {
    anyhow::ensure!(depth <= 8, "submodule watch depth exceeded");
    let root = repository
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no worktree"))?;
    plan.worktrees.push(root.to_path_buf());
    plan.file(root.to_path_buf())?;
    plan.file(root.join(".git"))?;
    collect_worktree(repository, plan, cancellation, visited)?;
    collect_metadata(repository, plan, cancellation, visited)?;
    if let Some(submodules) = repository.submodules()? {
        for submodule in submodules {
            if let Some(repository) = submodule.open()? {
                collect_repository(&repository, plan, cancellation, visited, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn collect_worktree(
    repository: &gix::Repository,
    plan: &mut Plan,
    cancellation: &AtomicBool,
    visited: &mut usize,
) -> anyhow::Result<()> {
    let root = repository
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("repository has no worktree"))?;
    let index = repository.index_or_empty()?;
    let mut excludes = repository.excludes(
        &index,
        None,
        gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
    )?;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        anyhow::ensure!(
            !cancellation.load(Ordering::Relaxed),
            "watch setup cancelled"
        );
        plan.directory(directory.clone())?;
        for entry in std::fs::read_dir(&directory)? {
            *visited += 1;
            anyhow::ensure!(
                !cancellation.load(Ordering::Relaxed)
                    && (!(*visited).is_multiple_of(256) || plan.bytes() <= PLAN_BYTES_MAX),
                "watch setup budget exceeded"
            );
            anyhow::ensure!(
                plan.ignored.len() + plan.files.len() < 1024,
                "watch filter budget exceeded"
            );
            anyhow::ensure!(
                *visited <= PLAN_ENTRIES_MAX,
                "watch traversal budget exceeded"
            );
            let entry = entry?;
            let path = entry.path();
            if entry.file_name() == ".git" {
                plan.ignored.insert(path);
                continue;
            }
            let relative = path.strip_prefix(root)?;
            let relative_bytes = gix::path::into_bstr(relative);
            let tracked_directory = index.path_is_directory(relative_bytes.as_bstr());
            let directory = entry.file_type()?.is_dir();
            let tracked =
                tracked_directory || index.entry_by_path(relative_bytes.as_bstr()).is_some();
            let ignored = !tracked
                && excludes
                    .at_path(relative, directory.then_some(gix::index::entry::Mode::DIR))?
                    .is_excluded();
            if ignored {
                plan.ignored.insert(path);
                continue;
            }
            if directory && !path.join(".git").exists() {
                pending.push(path);
                anyhow::ensure!(
                    pending.len() + plan.directories.len() <= WATCHES_PER_PROJECT_MAX,
                    "watch traversal budget exceeded"
                );
            }
        }
    }
    Ok(())
}

fn collect_metadata(
    repository: &gix::Repository,
    plan: &mut Plan,
    cancellation: &AtomicBool,
    visited: &mut usize,
) -> anyhow::Result<()> {
    for metadata in [repository.git_dir(), repository.common_dir()] {
        let metadata = metadata.canonicalize()?;
        plan.directory(metadata.clone())?;
        for name in [
            "HEAD",
            "index",
            "config",
            "config.worktree",
            "packed-refs",
            "shallow",
            "commondir",
            "gitdir",
        ] {
            plan.file(metadata.join(name))?;
        }
        plan.file(metadata.join("info/exclude"))?;
        let refs = metadata.join("refs");
        plan.metadata.insert(refs.clone());
        collect_refs(refs, plan, cancellation, visited)?;
    }
    plan.file(repository.index_path())?;
    let config = repository.config_snapshot();
    for section in config.plumbing().sections() {
        if let Some(path) = &section.meta().path {
            plan.file(path.clone())?;
        }
    }
    if let Some(path) = config.trusted_path("core.excludesFile")? {
        plan.file(path)?;
    }
    if let Some(base) = directories::BaseDirs::new() {
        plan.file(base.config_dir().join("git/ignore"))?;
        plan.file(base.home_dir().join(".gitconfig"))?;
    }
    Ok(())
}

fn collect_refs(
    refs: PathBuf,
    plan: &mut Plan,
    cancellation: &AtomicBool,
    visited: &mut usize,
) -> anyhow::Result<()> {
    let mut refs_pending = vec![refs];
    while let Some(directory) = refs_pending.pop() {
        if !directory.is_dir() {
            continue;
        }
        plan.directory(directory.clone())?;
        for entry in std::fs::read_dir(directory)? {
            *visited += 1;
            anyhow::ensure!(
                *visited <= PLAN_ENTRIES_MAX && !cancellation.load(Ordering::Relaxed),
                "metadata watch traversal stopped"
            );
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                refs_pending.push(entry.path());
            }
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn symlinked_roots_accept_canonical_events_for_present_and_missing_repositories() {
        let temporary = tempfile::tempdir().unwrap();
        let actual = temporary.path().join("actual");
        std::fs::create_dir(&actual).unwrap();
        let alias = temporary.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let actual = actual.canonicalize().unwrap();

        let existing = plan(&alias, &AtomicBool::new(false)).unwrap();
        assert!(existing.accepts(&actual.join(".git")));
        assert!(existing.directories.contains(&actual));

        let missing = plan(&alias.join("new-project"), &AtomicBool::new(false)).unwrap();
        assert!(missing.accepts(&actual.join("new-project")));
        assert!(missing.directories.contains(&actual));
    }

    #[test]
    fn linked_worktrees_accept_canonical_events_for_aliased_git_metadata() {
        let temporary = tempfile::tempdir().unwrap();
        let actual = temporary.path().join("actual");
        std::fs::create_dir(&actual).unwrap();
        let actual = actual.canonicalize().unwrap();
        let alias = temporary.path().join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let linked = temporary.path().join("linked");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.test",
                "commit",
                "--allow-empty",
                "-qm",
                "initial",
            ],
            vec![
                "worktree",
                "add",
                "-qb",
                "feature",
                linked.to_str().unwrap(),
            ],
        ] {
            let output = std::process::Command::new("git")
                .current_dir(&actual)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        std::fs::write(
            linked.join(".git"),
            format!("gitdir: {}/.git/worktrees/linked\n", alias.display()),
        )
        .unwrap();

        let watch = plan(&linked, &AtomicBool::new(false)).unwrap();
        assert!(watch.accepts(&actual.join(".git/worktrees/linked/index")));
        assert!(watch.accepts(&actual.join(".git/refs/heads/another")));
    }
}
