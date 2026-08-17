use std::{num::NonZeroU32, path::Path};

use anyhow::Context as _;
use gtl_application::{
    diffs::{
        DiffTarget, DiffTargetRequest, RepoRef,
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_diff_subrepos::RenderDiffSubrepos,
    },
    ports::DiffRenderRequest,
    projects::render_project_diff::RenderProjectDiff,
    repositories::find_repository_roots,
};
use gtl_models::{
    paths::RepositoryRoot,
    repository::traversal::{RepositoryTarget, RepositoryTraversalScope},
};

use crate::commands::diff::DiffOutcome;

pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let root = canonical_root(root.as_ref())?;
    let repos = scan_repo_tops(&root, include_worktrees)?;
    if repos.is_empty() {
        anyhow::bail!("diff -r: no git repos found under {}", root.display());
    }
    let target = last.map_or(DiffTarget::Unpushed { pinned: None }, |count| {
        DiffTarget::Last {
            count,
            pinned: None,
        }
    });
    let recipe_intents = repos
        .iter()
        .map(|repo| DiffRecipeIntent {
            repo_root: repo.path.clone(),
            operation: crate::recipe::diff_operation(&target),
            name: Some(repo.label.clone()),
        })
        .collect();
    let repo_refs = to_repo_refs(repos);
    super::present(PresentDiff {
        render: DiffRenderRequest::Subrepos(RenderDiffSubrepos {
            root,
            target: last.map_or(DiffTargetRequest::Unpushed, |count| {
                DiffTargetRequest::Last { count: count.get() }
            }),
            repos: repo_refs,
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: recipe_intents,
        mode: super::presentation_mode(raw),
    })
}

pub fn run_managed_all(root: impl AsRef<Path>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let root = canonical_root(root.as_ref())?;
    let repos = crate::recipe::selected_managed_repos()?;
    let recipe_intents = repos
        .iter()
        .map(|repo| DiffRecipeIntent {
            repo_root: repo.path.clone(),
            operation: crate::recipe::diff_operation(&DiffTarget::Unpushed { pinned: None }),
            name: Some(repo.label.clone()),
        })
        .collect();
    super::present(PresentDiff {
        render: DiffRenderRequest::Projects(RenderProjectDiff {
            root,
            repos: to_repo_refs(repos),
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: recipe_intents,
        mode: super::presentation_mode(raw),
    })
}

fn canonical_root(root: &Path) -> anyhow::Result<RepositoryRoot> {
    let canonical = std::fs::canonicalize(root)
        .with_context(|| format!("failed to resolve {}", root.display()))?;
    RepositoryRoot::try_new(canonical).context("canonical scan root is not absolute")
}

fn scan_repo_tops(root: &Path, include_worktrees: bool) -> anyhow::Result<Vec<RepositoryTarget>> {
    let scope = if include_worktrees {
        RepositoryTraversalScope::IncludeLinkedWorktrees
    } else {
        RepositoryTraversalScope::ExcludeLinkedWorktrees
    };
    Ok(find_repository_roots::execute(
        find_repository_roots::FindRepositoryRoots {
            root: root.to_path_buf(),
            scope,
        },
        &gtl_infra::git_client::HybridGitClient,
    )?)
}

fn to_repo_refs(repos: Vec<RepositoryTarget>) -> Vec<RepoRef> {
    repos
        .into_iter()
        .map(|repo| RepoRef {
            top: repo.path,
            label: repo.label,
        })
        .collect()
}
