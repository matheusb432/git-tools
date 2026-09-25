use std::{
    cell::RefCell,
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const INDEX_BYTES_MAX: usize = 1024 * 1024;

#[derive(Default)]
struct Contexts {
    repositories: VecDeque<(PathBuf, gix::Repository, usize)>,
    index_bytes: usize,
    cancellation: Option<Arc<AtomicBool>>,
}

thread_local! {
    static CONTEXTS: RefCell<Contexts> = RefCell::new(Contexts::default());
}

pub struct StatusContextScope(std::marker::PhantomData<std::rc::Rc<()>>);

impl StatusContextScope {
    pub fn new(cancellation: Arc<AtomicBool>) -> Self {
        CONTEXTS.with_borrow_mut(|contexts| {
            *contexts = Contexts {
                cancellation: Some(cancellation),
                ..Default::default()
            };
        });
        Self(std::marker::PhantomData)
    }

    pub fn invalidate(path: &std::path::Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        CONTEXTS.with_borrow_mut(|contexts| {
            let index = contexts
                .repositories
                .iter()
                .position(|(cached, _, _)| cached == &path);
            if let Some((_, _, bytes)) = index.and_then(|index| contexts.repositories.remove(index))
            {
                contexts.index_bytes -= bytes;
            }
        });
    }
}

impl Drop for StatusContextScope {
    fn drop(&mut self) {
        CONTEXTS.with_borrow_mut(|contexts| *contexts = Contexts::default());
    }
}

pub(crate) fn cancellation() -> Option<Arc<AtomicBool>> {
    CONTEXTS.with_borrow(|contexts| contexts.cancellation.clone())
}

pub(super) fn with_repository<T>(
    path: &std::path::Path,
    read: impl FnOnce(&gix::Repository) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let interrupted = cancellation();
    anyhow::ensure!(
        !interrupted
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed)),
        "status cancelled"
    );
    let cached = CONTEXTS.with_borrow_mut(|contexts| {
        let index = contexts
            .repositories
            .iter()
            .position(|(cached, _, _)| cached == path)?;
        let (_, repository, bytes) = contexts.repositories.remove(index)?;
        contexts.index_bytes -= bytes;
        Some(repository)
    });
    let repository = match cached {
        Some(repository) => repository,
        None => gix::open(path)?,
    };
    let result = read(&repository);
    anyhow::ensure!(
        !interrupted
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed)),
        "status cancelled"
    );
    CONTEXTS.with_borrow_mut(|contexts| {
        if contexts
            .cancellation
            .as_ref()
            .is_none_or(|flag| flag.load(Ordering::Relaxed))
        {
            return;
        }
        let Ok(index) = repository.index_or_empty() else {
            return;
        };
        let bytes = std::mem::size_of_val(index.entries()) + index.path_backing().len();
        if bytes > INDEX_BYTES_MAX {
            return;
        }
        while contexts.index_bytes.saturating_add(bytes) > INDEX_BYTES_MAX
            || contexts.repositories.len() >= 15
        {
            let Some((_, _, bytes)) = contexts.repositories.pop_front() else {
                break;
            };
            contexts.index_bytes -= bytes;
        }
        contexts.index_bytes += bytes;
        contexts
            .repositories
            .push_back((path.to_path_buf(), repository, bytes));
    });
    result
}

#[cfg(test)]
mod tests {
    use gtl_application::ports::{GitClient as _, GitEffect};

    use super::*;
    use crate::testing::TestRepository;

    #[test]
    fn cached_status_observes_index_replacement_without_writing_it() -> anyhow::Result<()> {
        let repository = TestRepository::new();
        let root = repository.root();
        let index_path = repository.path().join(".git/index");
        repository.write("file", "initial");
        repository.commit_all("initial");
        let index_before = std::fs::read(&index_path).unwrap();
        let cancellation = Arc::new(AtomicBool::new(false));
        let scope = StatusContextScope::new(cancellation.clone());
        let GitEffect::Applied(first) = super::super::HybridGitClient
            .status_snapshot(&root)
            .unwrap()
        else {
            anyhow::bail!("status rejected")
        };
        repository.write("added", "new");
        repository.git(&["add", "added"]);
        let index_staged = std::fs::read(&index_path).unwrap();
        assert_ne!(index_before, index_staged);
        let GitEffect::Applied(staged) = super::super::HybridGitClient
            .status_snapshot(&root)
            .unwrap()
        else {
            anyhow::bail!("status rejected")
        };
        assert_ne!(first.working_tree, staged.working_tree);
        assert_eq!(std::fs::read(&index_path).unwrap(), index_staged);
        assert_eq!(
            CONTEXTS.with_borrow(|contexts| contexts.repositories.len()),
            1
        );
        cancellation.store(true, Ordering::Relaxed);
        assert!(
            super::super::HybridGitClient
                .status_snapshot(&root)
                .is_err()
        );
        drop(scope);
        assert_eq!(
            CONTEXTS.with_borrow(|contexts| contexts.repositories.len()),
            0
        );
        Ok(())
    }
}
