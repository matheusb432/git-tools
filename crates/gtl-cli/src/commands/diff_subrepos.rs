use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use gtl_application::{
    diffs::{
        DiffTarget, DiffTargetRequest, RepoRef,
        present_diff::{DiffRecipeIntent, PresentDiff},
        render_diff_all::RenderDiffAll,
        render_diff_subrepos::RenderDiffSubrepos,
    },
    ports::DiffRenderRequest,
    recipes::RecipeRequest,
};

use crate::{
    commands::{diff::DiffOutcome, managed::ManagedOptions},
    viewer,
};

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
            repo: repo.path.clone(),
            operation: RecipeRequest::Diff(target.clone()),
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
        raw,
        has_display: viewer::has_display(),
        effects_enabled: !viewer::no_open_requested(),
    })
}

pub fn run_managed_all(
    root: impl AsRef<Path>,
    options: &ManagedOptions,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let root = canonical_root(root.as_ref())?;
    let repos = crate::recipe::selected_managed_repos(options)?;
    let recipe_intents = repos
        .iter()
        .map(|repo| DiffRecipeIntent {
            repo: repo.path.clone(),
            operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
            name: Some(repo.label.clone()),
        })
        .collect();
    super::present(PresentDiff {
        render: DiffRenderRequest::ManagedAll(RenderDiffAll {
            root,
            repos: to_repo_refs(repos),
        }),
        batch_id: crate::recipe::new_batch_id(),
        recipes: recipe_intents,
        raw,
        has_display: viewer::has_display(),
        effects_enabled: !viewer::no_open_requested(),
    })
}

fn canonical_root(root: &Path) -> anyhow::Result<PathBuf> {
    std::fs::canonicalize(root).with_context(|| format!("failed to resolve {}", root.display()))
}

fn scan_repo_tops(
    root: &Path,
    include_worktrees: bool,
) -> anyhow::Result<Vec<gtl_models::discovery::DiscoveredRepo>> {
    Ok(gtl_application::discovery::find_repo_tops::execute(
        gtl_application::discovery::find_repo_tops::FindRepoTops {
            root: root.to_path_buf(),
            include_worktrees,
        },
        &gtl_infra::repo_discovery::WalkdirRepoDiscovery,
        &gtl_infra::git_client::HybridGitClient,
    )?)
}

fn to_repo_refs(repos: Vec<gtl_models::discovery::DiscoveredRepo>) -> Vec<RepoRef> {
    repos
        .into_iter()
        .map(|repo| RepoRef {
            top: repo.path.to_string_lossy().into_owned(),
            label: repo.label,
        })
        .collect()
}
