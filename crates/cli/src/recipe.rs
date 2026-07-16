//! Pure resolution from CLI inputs to [`Recipe`]s for the app-default viewer path.
//!
//! This module only resolves; it never dispatches to a backend or opens a viewer.
//! [`gtl_recipe`] is deliberately app-agnostic (no `domain` dependency), so the
//! `DiffTarget` to `RecipeTarget` mapping lives here, in the one crate that knows
//! about both.

use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use application::diffs::render_merge_diff::DEFAULT_BASE;
use gtl_recipe::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::{
    cli::DiffTarget,
    commands::{
        diff_subrepos::{scan_repo_tops, unpushed_managed_repo_tops},
        managed::ManagedOptions,
    },
    git,
};

/// Carry an optional pin across the crate boundary: `application::diffs::PinnedRange`
/// to `gtl_recipe::PinnedRange`.
fn to_recipe_pin(
    pinned: Option<&application::diffs::PinnedRange>,
) -> Option<gtl_recipe::PinnedRange> {
    pinned.map(|pin| gtl_recipe::PinnedRange {
        base: pin.base.clone(),
        head: pin.head.clone(),
    })
}

/// Map the application [`DiffTarget`] onto the wire-agnostic [`RecipeTarget`] — the
/// inverse of the viewer's recipe-to-domain mapping.
fn recipe_target_from_diff_target(target: &DiffTarget) -> RecipeTarget {
    match target {
        DiffTarget::Unpushed { pinned } => RecipeTarget::Unpushed {
            pinned: to_recipe_pin(pinned.as_ref()),
        },
        DiffTarget::Base(rev) => RecipeTarget::Base { rev: rev.clone() },
        DiffTarget::Range { range, pinned } => RecipeTarget::Range {
            range: range.clone(),
            pinned: to_recipe_pin(pinned.as_ref()),
        },
        DiffTarget::Merge { base, pinned } => RecipeTarget::Merge {
            base: base.clone(),
            pinned: to_recipe_pin(pinned.as_ref()),
        },
        DiffTarget::Last { count, pinned } => RecipeTarget::Last {
            count: *count,
            pinned: to_recipe_pin(pinned.as_ref()),
        },
    }
}

/// Wrap a [`DiffTarget`] into the `diff` family's [`RecipeOp`].
pub(crate) fn diff_op_from_target(target: &DiffTarget) -> RecipeOp {
    RecipeOp::Diff {
        target: recipe_target_from_diff_target(target),
    }
}

/// Resolve `cwd`'s git top-level into a single-repo [`Recipe`] for `op` and `name`.
pub(crate) fn recipe_for_cwd(
    cwd: &Path,
    op: RecipeOp,
    name: Option<&str>,
) -> anyhow::Result<Recipe> {
    let top = git::top_level(cwd)?;
    let op = pin_op(Path::new(&top), op);
    Ok(Recipe {
        source: RecipeSource::LocalRepo(PathBuf::from(top)),
        op,
        name: name.map(str::to_string),
    })
}

/// Resolve `op`'s symbolic range into pinned SHAs at mint time (GTL-0131):
/// snapshot recipes must carry the data of *this* invocation. Resolution
/// failures mint the symbolic op unchanged — pinning never fails a command —
/// and unpinnable shapes (worktree bases, three-dot ranges) stay symbolic.
pub(crate) fn pin_op(repo_top: &Path, op: RecipeOp) -> RecipeOp {
    match op {
        RecipeOp::Diff { target } => RecipeOp::Diff {
            target: pin_target(repo_top, target),
        },
        RecipeOp::MergeDiff { base, pinned: None } => {
            let pinned = pin_merge(repo_top, base.as_deref());
            RecipeOp::MergeDiff { base, pinned }
        }
        RecipeOp::SquashPreview { pinned: None } => RecipeOp::SquashPreview {
            pinned: pin_range(repo_top, "@{u}", "HEAD"),
        },
        op => op,
    }
}

fn pin_target(repo_top: &Path, target: RecipeTarget) -> RecipeTarget {
    match target {
        RecipeTarget::Unpushed { pinned: None } => RecipeTarget::Unpushed {
            pinned: pin_range(repo_top, "@{u}", "HEAD"),
        },
        RecipeTarget::Last {
            count,
            pinned: None,
        } => RecipeTarget::Last {
            count,
            pinned: pin_range(repo_top, &format!("HEAD~{count}"), "HEAD"),
        },
        RecipeTarget::Range {
            range,
            pinned: None,
        } => {
            let pinned = pin_exact_range(repo_top, &range);
            RecipeTarget::Range { range, pinned }
        }
        RecipeTarget::Merge { base, pinned: None } => {
            let pinned = pin_merge(repo_top, Some(&base));
            RecipeTarget::Merge { base, pinned }
        }
        target => target,
    }
}

fn pin_range(repo_top: &Path, base: &str, head: &str) -> Option<PinnedRange> {
    Some(PinnedRange {
        base: git::resolve_sha(repo_top, base).ok()?,
        head: git::resolve_sha(repo_top, head).ok()?,
    })
}

fn pin_exact_range(repo_top: &Path, range: &str) -> Option<PinnedRange> {
    if range.contains("...") {
        return None; // three-dot semantics are not a plain endpoint pair
    }
    let (base, head) = range.split_once("..")?;
    if base.is_empty() || head.is_empty() {
        return None;
    }
    pin_range(repo_top, base, head)
}

fn pin_merge(repo_top: &Path, base: Option<&str>) -> Option<PinnedRange> {
    let base = base
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or(DEFAULT_BASE);
    Some(PinnedRange {
        base: git::merge_base(repo_top, base, "HEAD").ok()?,
        head: git::resolve_sha(repo_top, "HEAD").ok()?,
    })
}

/// Mint a fresh batch id for a group of recipes opened together.
pub(crate) fn new_batch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Fan out `diff --all`'s managed-repo, unpushed-only pre-filter into one `Diff {
/// target: Unpushed }` recipe per changed repo — reusing
/// [`unpushed_managed_repo_tops`], the exact pre-filter `run_managed_all_with` uses.
pub(crate) fn managed_recipes(
    root: &Path,
    options: &ManagedOptions,
) -> anyhow::Result<Vec<Recipe>> {
    std::fs::canonicalize(root)
        .map_err(|error| anyhow::anyhow!("failed to resolve {}: {error}", root.display()))?;
    let tops = unpushed_managed_repo_tops(options)?;
    Ok(tops
        .into_iter()
        .map(|repo_top| Recipe {
            op: pin_op(
                &repo_top.path,
                RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            ),
            source: RecipeSource::LocalRepo(repo_top.path),
            name: Some(repo_top.label),
        })
        .collect())
}

/// Fan out `diff -r`'s repo discovery under `root` into one `Diff` recipe per
/// discovered repo — reusing [`scan_repo_tops`], the exact discovery
/// `run_scan_with` uses. `last` maps like the raw path: absent is `Unpushed`,
/// present is `Last { count }`.
pub(crate) fn subrepo_recipes(
    root: &Path,
    last: Option<NonZeroU32>,
    worktrees: bool,
) -> anyhow::Result<Vec<Recipe>> {
    let root = std::fs::canonicalize(root)
        .map_err(|error| anyhow::anyhow!("failed to resolve {}: {error}", root.display()))?;
    let target = recipe_target_from_diff_target(&last.map_or(
        DiffTarget::Unpushed { pinned: None },
        |count| DiffTarget::Last {
            count,
            pinned: None,
        },
    ));
    let tops = scan_repo_tops(&root, worktrees)?;
    Ok(tops
        .into_iter()
        .map(|repo_top| Recipe {
            op: pin_op(
                &repo_top.path,
                RecipeOp::Diff {
                    target: target.clone(),
                },
            ),
            source: RecipeSource::LocalRepo(repo_top.path),
            name: Some(repo_top.label),
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
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "first"]);
    }

    #[test]
    fn diff_op_from_target_maps_every_variant() {
        assert_eq!(
            diff_op_from_target(&DiffTarget::Unpushed { pinned: None }),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
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
            diff_op_from_target(&DiffTarget::Range {
                range: "a..b".to_string(),
                pinned: None
            }),
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: "a..b".to_string(),
                    pinned: None
                }
            }
        );
        assert_eq!(
            diff_op_from_target(&DiffTarget::Merge {
                base: "main".to_string(),
                pinned: None
            }),
            RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: "main".to_string(),
                    pinned: None
                }
            }
        );
        let count = NonZeroU32::new(3).unwrap();
        assert_eq!(
            diff_op_from_target(&DiffTarget::Last {
                count,
                pinned: None
            }),
            RecipeOp::Diff {
                target: RecipeTarget::Last {
                    count,
                    pinned: None
                }
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
                target: RecipeTarget::Unpushed { pinned: None },
            },
            None,
        )
        .unwrap();

        assert_eq!(recipe.source, RecipeSource::LocalRepo(canonical_top));
        assert_eq!(
            recipe.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }

    #[test]
    fn recipe_for_cwd_errors_outside_a_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let result = recipe_for_cwd(tmp.path(), RecipeOp::SquashPreview { pinned: None }, None);
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
                    target: RecipeTarget::Unpushed { pinned: None }
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
                target: RecipeTarget::Last {
                    count,
                    pinned: None
                }
            }
        );
    }

    fn git_out(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// `init_repo` + a bare origin with the first commit pushed as upstream, then
    /// one extra unpushed commit. Returns `(upstream_sha, head_sha)`.
    fn init_repo_with_upstream(dir: &Path) -> (String, String) {
        init_repo(dir);
        let bare = dir.join("origin.git");
        git_out(dir, &["init", "--bare", "-q", bare.to_str().unwrap()]);
        git_out(dir, &["remote", "add", "origin", bare.to_str().unwrap()]);
        let branch = git_out(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
        git_out(dir, &["push", "-q", "-u", "origin", &branch]);
        let upstream_sha = git_out(dir, &["rev-parse", "HEAD"]);
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        git_out(dir, &["add", "."]);
        git_out(dir, &["commit", "-qm", "second"]);
        let head_sha = git_out(dir, &["rev-parse", "HEAD"]);
        (upstream_sha, head_sha)
    }

    #[test]
    fn recipe_for_cwd_pins_unpushed_to_the_resolved_upstream_and_head() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, head) = init_repo_with_upstream(tmp.path());

        let recipe = recipe_for_cwd(
            tmp.path(),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            None,
        )
        .unwrap();

        assert_eq!(
            recipe.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(gtl_recipe::PinnedRange { base, head })
                }
            }
        );
    }

    #[test]
    fn recipe_for_cwd_without_an_upstream_stays_unpinned() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path()); // no remote, no upstream

        let recipe = recipe_for_cwd(
            tmp.path(),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            None,
        )
        .unwrap();

        assert_eq!(
            recipe.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }

    #[test]
    fn recipe_for_cwd_pins_last_n_to_the_resolved_endpoints() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, head) = init_repo_with_upstream(tmp.path()); // 2 commits total
        let base = git_out(tmp.path(), &["rev-parse", "HEAD~1"]);

        let recipe = recipe_for_cwd(
            tmp.path(),
            RecipeOp::Diff {
                target: RecipeTarget::Last {
                    count: NonZeroU32::new(1).unwrap(),
                    pinned: None,
                },
            },
            None,
        )
        .unwrap();

        assert_eq!(
            recipe.op,
            RecipeOp::Diff {
                target: RecipeTarget::Last {
                    count: NonZeroU32::new(1).unwrap(),
                    pinned: Some(gtl_recipe::PinnedRange { base, head }),
                }
            }
        );
    }

    #[test]
    fn recipe_for_cwd_pins_a_two_dot_range_and_skips_three_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, head) = init_repo_with_upstream(tmp.path());
        let range = format!("{base}..{head}");

        let pinned = recipe_for_cwd(
            tmp.path(),
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: range.clone(),
                    pinned: None,
                },
            },
            None,
        )
        .unwrap();
        assert_eq!(
            pinned.op,
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range,
                    pinned: Some(gtl_recipe::PinnedRange {
                        base: base.clone(),
                        head: head.clone()
                    }),
                }
            }
        );

        let three_dot = format!("{base}...{head}");
        let symbolic = recipe_for_cwd(
            tmp.path(),
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: three_dot.clone(),
                    pinned: None,
                },
            },
            None,
        )
        .unwrap();
        assert_eq!(
            symbolic.op,
            RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: three_dot,
                    pinned: None
                }
            }
        );
    }

    #[test]
    fn recipe_for_cwd_pins_merge_diff_to_merge_base_and_head() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        git_out(tmp.path(), &["checkout", "-qb", "feature"]);
        std::fs::write(tmp.path().join("f.txt"), "f\n").unwrap();
        git_out(tmp.path(), &["add", "."]);
        git_out(tmp.path(), &["commit", "-qm", "feature work"]);
        let merge_base = git_out(tmp.path(), &["merge-base", "main", "HEAD"]);
        let head = git_out(tmp.path(), &["rev-parse", "HEAD"]);

        let recipe = recipe_for_cwd(
            tmp.path(),
            RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            None,
        )
        .unwrap();

        assert_eq!(
            recipe.op,
            RecipeOp::MergeDiff {
                base: None,
                pinned: Some(gtl_recipe::PinnedRange {
                    base: merge_base,
                    head
                }),
            }
        );
    }

    #[test]
    fn recipe_for_cwd_pins_squash_preview_like_unpushed() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, head) = init_repo_with_upstream(tmp.path());

        let recipe =
            recipe_for_cwd(tmp.path(), RecipeOp::SquashPreview { pinned: None }, None).unwrap();

        assert_eq!(
            recipe.op,
            RecipeOp::SquashPreview {
                pinned: Some(gtl_recipe::PinnedRange { base, head })
            }
        );
    }

    #[test]
    fn subrepo_recipes_pin_each_repo_independently() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("api")).unwrap();
        let (api_base, api_head) = init_repo_with_upstream(&root.join("api"));
        std::fs::create_dir_all(root.join("web")).unwrap();
        init_repo(&root.join("web")); // no upstream — stays unpinned

        let recipes = subrepo_recipes(root, None, false).unwrap();

        let api = recipes
            .iter()
            .find(|r| r.name.as_deref() == Some("api"))
            .unwrap();
        assert_eq!(
            api.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(gtl_recipe::PinnedRange {
                        base: api_base,
                        head: api_head
                    }),
                }
            }
        );
        let web = recipes
            .iter()
            .find(|r| r.name.as_deref() == Some("web"))
            .unwrap();
        assert_eq!(
            web.op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }
}
