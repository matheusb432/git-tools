//! Pure resolution from CLI inputs to [`Recipe`]s — the Phase 5 default render
//! path forwards these to the viewer instead of writing a store artifact itself.
//!
//! This module only resolves; it never dispatches to a backend or opens a viewer
//! (that's Tasks 1.5-1.8). [`gtl_recipe`] is deliberately app-agnostic (no `domain`
//! dependency), so the `DiffTarget` -> `RecipeTarget` mapping lives here, in the
//! one crate that knows about both.

use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use gtl_recipe::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::{
    cli::DiffTarget,
    commands::{
        diff_subrepos::{scan_repo_tops, unpushed_managed_repo_tops},
        managed::ManagedOptions,
    },
    git,
};

/// Map the domain [`DiffTarget`] onto the wire-agnostic [`RecipeTarget`] — the
/// reverse of `crates/desktop/src/commands/tabs.rs`'s `diff_target`.
fn recipe_target_from_diff_target(target: &DiffTarget) -> RecipeTarget {
    match target {
        DiffTarget::Unpushed => RecipeTarget::Unpushed,
        DiffTarget::Base(rev) => RecipeTarget::Base { rev: rev.clone() },
        DiffTarget::Range(range) => RecipeTarget::Range {
            range: range.clone(),
        },
        DiffTarget::Merge(base) => RecipeTarget::Merge { base: base.clone() },
        DiffTarget::Last(count) => RecipeTarget::Last { count: *count },
    }
}

/// Wrap a [`DiffTarget`] into the `diff` family's [`RecipeOp`].
pub fn diff_op_from_target(target: &DiffTarget) -> RecipeOp {
    RecipeOp::Diff {
        target: recipe_target_from_diff_target(target),
    }
}

/// Resolve `cwd`'s git top-level into a single-repo [`Recipe`] for `op`.
pub fn recipe_for_cwd(cwd: &Path, op: RecipeOp) -> anyhow::Result<Recipe> {
    let top = git::top_level(cwd)?;
    Ok(Recipe {
        source: RecipeSource::LocalRepo(PathBuf::from(top)),
        op,
    })
}

/// Mint a fresh batch id for a group of recipes opened together.
pub fn new_batch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Fan out `diff --all`'s managed-repo, unpushed-only pre-filter into one `Diff {
/// target: Unpushed }` recipe per changed repo — reusing
/// [`unpushed_managed_repo_tops`], the exact pre-filter `run_managed_all_with` uses.
pub fn managed_recipes(root: &Path, options: &ManagedOptions) -> anyhow::Result<Vec<Recipe>> {
    std::fs::canonicalize(root)
        .map_err(|error| anyhow::anyhow!("failed to resolve {}: {error}", root.display()))?;
    let tops = unpushed_managed_repo_tops(options)?;
    Ok(tops
        .into_iter()
        .map(|repo_top| Recipe {
            source: RecipeSource::LocalRepo(repo_top.top),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed,
            },
        })
        .collect())
}

/// Fan out `diff -r`'s repo discovery under `root` into one `Diff` recipe per
/// discovered repo — reusing [`scan_repo_tops`], the exact discovery
/// `run_scan_with` uses. `last` maps like the raw path: absent is `Unpushed`,
/// present is `Last { count }`.
pub fn subrepo_recipes(
    root: &Path,
    last: Option<NonZeroU32>,
    worktrees: bool,
) -> anyhow::Result<Vec<Recipe>> {
    let root = std::fs::canonicalize(root)
        .map_err(|error| anyhow::anyhow!("failed to resolve {}: {error}", root.display()))?;
    let target =
        recipe_target_from_diff_target(&last.map_or(DiffTarget::Unpushed, DiffTarget::Last));
    let tops = scan_repo_tops(&root, worktrees)?;
    Ok(tops
        .into_iter()
        .map(|repo_top| Recipe {
            source: RecipeSource::LocalRepo(repo_top.top),
            op: RecipeOp::Diff {
                target: target.clone(),
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) {
        let g = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "first"]);
    }

    #[test]
    fn diff_op_from_target_maps_every_variant() {
        assert_eq!(
            diff_op_from_target(&DiffTarget::Unpushed),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed
            }
        );
        assert_eq!(
            diff_op_from_target(&DiffTarget::Base("main".to_string())),
            RecipeOp::Diff {
                target: RecipeTarget::Base {
                    rev: "main".to_string()
                }
            }
        );
        assert_eq!(
            diff_op_from_target(&DiffTarget::Range("a..b".to_string())),
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: "a..b".to_string()
                }
            }
        );
        assert_eq!(
            diff_op_from_target(&DiffTarget::Merge("main".to_string())),
            RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: "main".to_string()
                }
            }
        );
        let count = NonZeroU32::new(3).unwrap();
        assert_eq!(
            diff_op_from_target(&DiffTarget::Last(count)),
            RecipeOp::Diff {
                target: RecipeTarget::Last { count }
            }
        );
    }

    #[test]
    fn recipe_for_cwd_resolves_local_repo_top_level() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let canonical_top = std::fs::canonicalize(tmp.path()).unwrap();

        let recipe = recipe_for_cwd(
            tmp.path(),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed,
            },
        )
        .unwrap();

        assert_eq!(recipe.source, RecipeSource::LocalRepo(canonical_top));
        assert_eq!(
            recipe.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed
            }
        );
    }

    #[test]
    fn recipe_for_cwd_errors_outside_a_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let result = recipe_for_cwd(tmp.path(), RecipeOp::SquashPreview);
        assert!(result.is_err());
    }

    #[test]
    fn new_batch_id_yields_distinct_uuids() {
        assert_ne!(new_batch_id(), new_batch_id());
    }

    #[test]
    fn subrepo_recipes_yields_one_diff_recipe_per_discovered_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("api")).unwrap();
        init_repo(&root.join("api"));
        std::fs::create_dir_all(root.join("web")).unwrap();
        init_repo(&root.join("web"));

        let recipes = subrepo_recipes(root, None, false).unwrap();

        assert_eq!(recipes.len(), 2);
        for recipe in &recipes {
            assert_eq!(
                recipe.op,
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed
                }
            );
        }
    }

    #[test]
    fn managed_recipes_skips_repos_without_an_upstream() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join("repo1")).unwrap();
        init_repo(&home.join("repo1")); // no remote configured -> no upstream

        let manifest = tmp.path().join("repos.toml");
        std::fs::write(
            &manifest,
            "[[repo]]\npath = \"repo1\"\nremote = \"origin\"\n",
        )
        .unwrap();

        let options = ManagedOptions {
            repos_file: Some(manifest),
            home_dir: Some(home),
            dry: false,
            json: false,
            color: false,
            message_for_all: None,
            interactive: false,
        };

        let recipes = managed_recipes(tmp.path(), &options).unwrap();

        assert!(recipes.is_empty(), "repo without upstream must be skipped");
    }

    #[test]
    fn subrepo_recipes_maps_last_count_onto_every_recipe() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("api")).unwrap();
        init_repo(&root.join("api"));
        let count = NonZeroU32::new(5).unwrap();

        let recipes = subrepo_recipes(root, Some(count), false).unwrap();

        assert_eq!(recipes.len(), 1);
        assert_eq!(
            recipes[0].op,
            RecipeOp::Diff {
                target: RecipeTarget::Last { count }
            }
        );
    }
}
