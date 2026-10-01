//! Narrows a diff to the changes committed after a cutoff.

use std::collections::{HashMap, HashSet};

use gtl_models::{
    diffs::{Commit, CommitId},
    git::{GitDiffSpec, GitRange, GitRevision},
    timestamps::MachineTimestamp,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NarrowedChanges {
    pub(super) spec: GitDiffSpec,
    pub(super) commits: Vec<Commit>,
}

/// Starts the diff at the newest first-parent commit older than `cutoff`.
///
/// Returns `None` when every first-parent commit in `commits` is at or after `cutoff`, so the
/// whole range remains. Working-tree specs keep comparing against the working tree.
pub(super) fn narrow(
    spec: &GitDiffSpec,
    commits: &[Commit],
    cutoff: &MachineTimestamp,
) -> Option<NarrowedChanges> {
    let by_id = commits
        .iter()
        .map(|commit| (&commit.id, commit))
        .collect::<HashMap<_, _>>();
    let tip = tip(commits)?;
    let mut current = tip;
    let mut boundary = None;
    for _ in 0..commits.len() {
        if &current.committed_at < cutoff {
            boundary = Some(current);
            break;
        }
        let Some(parent) = current.parents.first().and_then(|parent| by_id.get(parent)) else {
            break;
        };
        current = parent;
    }
    let boundary = boundary?;
    let boundary_revision = GitRevision::from(&boundary.id);
    let spec = match spec {
        GitDiffSpec::AgainstWorkingTree(_) => GitDiffSpec::AgainstWorkingTree(boundary_revision),
        GitDiffSpec::Range(_) => GitDiffSpec::Range(GitRange::two_dot(
            &boundary_revision,
            &GitRevision::from(&tip.id),
        )),
    };
    let excluded = ancestors(&boundary.id, &by_id);
    Some(NarrowedChanges {
        spec,
        commits: commits
            .iter()
            .filter(|commit| !excluded.contains(&&commit.id))
            .cloned()
            .collect(),
    })
}

fn tip(commits: &[Commit]) -> Option<&Commit> {
    let parents = commits
        .iter()
        .flat_map(|commit| &commit.parents)
        .collect::<HashSet<_>>();
    commits.iter().find(|commit| !parents.contains(&commit.id))
}

fn ancestors<'a>(
    start: &'a CommitId,
    by_id: &HashMap<&'a CommitId, &'a Commit>,
) -> HashSet<&'a CommitId> {
    let mut seen = HashSet::from([start]);
    let mut pending = vec![start];
    while let Some(id) = pending.pop() {
        let Some(commit) = by_id.get(id) else {
            continue;
        };
        for parent in &commit.parents {
            if by_id.contains_key(parent) && seen.insert(parent) {
                pending.push(parent);
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{commit_id_fixture, diffs::commit_with};

    fn at(commit: Commit, timestamp: &str) -> Commit {
        Commit {
            committed_at: MachineTimestamp::try_from(timestamp).unwrap(),
            ..commit
        }
    }

    /// `d` merges side commit `s` into `c`; log order lists the newest first.
    fn history() -> Vec<Commit> {
        vec![
            at(
                commit_with("dddd", "d", &["cccc", "5555"]),
                "2026-09-28T12:00:00Z",
            ),
            at(commit_with("5555", "s", &["aaaa"]), "2026-09-27T09:00:00Z"),
            at(commit_with("cccc", "c", &["bbbb"]), "2026-09-28T09:00:00Z"),
            at(commit_with("bbbb", "b", &["aaaa"]), "2026-09-27T12:00:00Z"),
            at(commit_with("aaaa", "a", &["0000"]), "2026-09-26T12:00:00Z"),
        ]
    }

    fn cutoff(timestamp: &str) -> MachineTimestamp {
        MachineTimestamp::try_from(timestamp).unwrap()
    }

    fn ids(commits: &[Commit]) -> Vec<String> {
        commits
            .iter()
            .map(|commit| commit.subject.clone())
            .collect()
    }

    #[test]
    fn range_starts_at_the_newest_first_parent_commit_before_the_cutoff() {
        let spec = GitDiffSpec::Range(GitRange::try_new("main...HEAD").unwrap());

        let narrowed = narrow(&spec, &history(), &cutoff("2026-09-28T00:00:00Z")).unwrap();

        assert_eq!(
            narrowed.spec.as_arg(),
            format!(
                "{}..{}",
                commit_id_fixture("bbbb"),
                commit_id_fixture("dddd")
            )
        );
        assert_eq!(ids(&narrowed.commits), ["d", "s", "c"]);
    }

    #[test]
    fn working_tree_specs_keep_comparing_against_the_working_tree() {
        let spec = GitDiffSpec::AgainstWorkingTree(GitRevision::main());

        let narrowed = narrow(&spec, &history(), &cutoff("2026-09-29T00:00:00Z")).unwrap();

        assert_eq!(
            narrowed.spec,
            GitDiffSpec::AgainstWorkingTree(GitRevision::from(&commit_id_fixture("dddd")))
        );
        assert_eq!(narrowed.commits, Vec::<gtl_models::diffs::Commit>::new());
    }

    #[test]
    fn a_cutoff_before_every_first_parent_commit_keeps_the_whole_range() {
        let spec = GitDiffSpec::Range(GitRange::try_new("main..HEAD").unwrap());

        assert_eq!(
            narrow(&spec, &history(), &cutoff("2026-09-01T00:00:00Z")),
            None
        );
        assert_eq!(narrow(&spec, &[], &cutoff("2026-09-01T00:00:00Z")), None);
    }
}
