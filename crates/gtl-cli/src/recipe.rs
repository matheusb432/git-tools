//! Recipe batch identity and managed-repository selection.

use gtl_application::diffs::{DiffTarget, PinnedRange};
use gtl_models::{discovery::DiscoveredRepo, managed::ManagedRepo};
use gtl_wire::recipes::{RecipeOp, RecipeTarget};

use crate::commands::managed;

/// Mints a fresh batch identifier for recipes opened together.
pub(crate) fn new_batch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(crate) fn diff_operation(target: &DiffTarget) -> RecipeOp {
    RecipeOp::Diff {
        target: match target {
            DiffTarget::Unpushed { pinned } => RecipeTarget::Unpushed {
                pinned: pinned.as_ref().map(to_wire_pin),
            },
            DiffTarget::Base(rev) => RecipeTarget::Base { rev: rev.clone() },
            DiffTarget::Range { range, pinned } => RecipeTarget::Range {
                range: range.clone(),
                pinned: pinned.as_ref().map(to_wire_pin),
            },
            DiffTarget::Merge { base, pinned } => RecipeTarget::Merge {
                base: base.clone(),
                pinned: pinned.as_ref().map(to_wire_pin),
            },
            DiffTarget::Last { count, pinned } => RecipeTarget::Last {
                count: *count,
                pinned: pinned.as_ref().map(to_wire_pin),
            },
        },
    }
}

fn to_wire_pin(pin: &PinnedRange) -> gtl_wire::recipes::PinnedRange {
    gtl_wire::recipes::PinnedRange {
        base: pin.base.clone(),
        head: pin.head.clone(),
    }
}

/// Loads sample_project's active projects and selects repositories with unpushed commits.
pub(crate) fn selected_managed_repos() -> anyhow::Result<Vec<DiscoveredRepo>> {
    let repos = managed::load_projects()?;
    select_managed_repos(repos)
}

fn select_managed_repos(repos: Vec<ManagedRepo>) -> anyhow::Result<Vec<DiscoveredRepo>> {
    Ok(gtl_application::managed::select_unpushed::execute(
        gtl_application::managed::select_unpushed::SelectUnpushed { repos },
        &gtl_infra::git_client::HybridGitClient,
    )?)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_application::diffs::{DiffTarget, PinnedRange};
    use gtl_wire::recipes::{RecipeOp, RecipeTarget};

    use super::{diff_operation, new_batch_id};

    #[test]
    fn new_batch_id_yields_distinct_uuids() {
        assert_ne!(new_batch_id(), new_batch_id());
    }

    #[test]
    fn diff_operation_preserves_every_target_field() {
        let pin = PinnedRange {
            base: "base".into(),
            head: "head".into(),
        };
        let wire_pin = gtl_wire::recipes::PinnedRange {
            base: "base".into(),
            head: "head".into(),
        };

        for (target, expected) in [
            (
                DiffTarget::Unpushed {
                    pinned: Some(pin.clone()),
                },
                RecipeTarget::Unpushed {
                    pinned: Some(wire_pin.clone()),
                },
            ),
            (
                DiffTarget::Base("main".into()),
                RecipeTarget::Base { rev: "main".into() },
            ),
            (
                DiffTarget::Range {
                    range: "main..HEAD".into(),
                    pinned: Some(pin.clone()),
                },
                RecipeTarget::Range {
                    range: "main..HEAD".into(),
                    pinned: Some(wire_pin.clone()),
                },
            ),
            (
                DiffTarget::Merge {
                    base: "release".into(),
                    pinned: Some(pin.clone()),
                },
                RecipeTarget::Merge {
                    base: "release".into(),
                    pinned: Some(wire_pin.clone()),
                },
            ),
            (
                DiffTarget::Last {
                    count: NonZeroU32::new(3).expect("positive count"),
                    pinned: Some(pin),
                },
                RecipeTarget::Last {
                    count: NonZeroU32::new(3).expect("positive count"),
                    pinned: Some(wire_pin),
                },
            ),
        ] {
            assert_eq!(diff_operation(&target), RecipeOp::Diff { target: expected });
        }
    }
}
