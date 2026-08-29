//! Closed status facts for one repository.

use serde::{Serialize, ser::SerializeStruct as _};

use crate::{
    git::{BranchName, CommitCount, GitRefName},
    paths::ProjectName,
    repository::PathCount,
};

/// The configured-upstream state for an attached branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusUpstream {
    Missing,
    Tracking {
        reference: GitRefName,
        ahead: CommitCount,
    },
}

/// The repository's checked-out head, including unavailable reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusHead {
    Unavailable,
    Detached,
    Branch {
        name: BranchName,
        upstream: StatusUpstream,
    },
}

/// Counts working-tree paths without conflating an unavailable status read with clean state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusChanges {
    Clean,
    Changed {
        tracked: PathCount,
        untracked: PathCount,
    },
    Unavailable,
}

impl StatusChanges {
    #[must_use]
    pub fn from_counts(tracked: PathCount, untracked: PathCount) -> Self {
        if tracked.is_zero() && untracked.is_zero() {
            Self::Clean
        } else {
            Self::Changed { tracked, untracked }
        }
    }

    #[must_use]
    pub const fn counts(self) -> (PathCount, PathCount) {
        match self {
            Self::Changed { tracked, untracked } => (tracked, untracked),
            Self::Clean | Self::Unavailable => (PathCount::new(0), PathCount::new(0)),
        }
    }

    #[must_use]
    pub const fn is_dirty(self) -> bool {
        matches!(self, Self::Changed { .. })
    }
}

/// The complete presence and status facts for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryStatus {
    Absent,
    Present {
        head: StatusHead,
        changes: StatusChanges,
    },
}

/// Stable high-level status classes used by JSON and terminal presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusClass {
    Absent,
    Clean,
    Pending,
    Warn,
}

impl StatusClass {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Clean => "clean",
            Self::Pending => "pending",
            Self::Warn => "warn",
        }
    }
}

/// One repository's status with every presentation fact derived from closed state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusResult {
    name: ProjectName,
    repository: RepositoryStatus,
}

impl StatusResult {
    #[must_use]
    pub fn absent(name: ProjectName) -> Self {
        Self {
            name,
            repository: RepositoryStatus::Absent,
        }
    }

    #[must_use]
    pub fn present(name: ProjectName, head: StatusHead, changes: StatusChanges) -> Self {
        Self {
            name,
            repository: RepositoryStatus::Present { head, changes },
        }
    }

    #[must_use]
    pub fn name(&self) -> &ProjectName {
        &self.name
    }

    #[must_use]
    pub const fn repository(&self) -> &RepositoryStatus {
        &self.repository
    }

    #[must_use]
    pub const fn is_present(&self) -> bool {
        matches!(self.repository, RepositoryStatus::Present { .. })
    }

    #[must_use]
    pub fn branch_label(&self) -> Option<&str> {
        match &self.repository {
            RepositoryStatus::Present {
                head: StatusHead::Detached,
                ..
            } => Some("detached"),
            RepositoryStatus::Present {
                head: StatusHead::Branch { name, .. },
                ..
            } => Some(name.as_ref()),
            RepositoryStatus::Absent
            | RepositoryStatus::Present {
                head: StatusHead::Unavailable,
                ..
            } => None,
        }
    }

    #[must_use]
    pub fn class(&self) -> StatusClass {
        let RepositoryStatus::Present { head, changes } = &self.repository else {
            return StatusClass::Absent;
        };
        if matches!(changes, StatusChanges::Unavailable)
            || matches!(
                head,
                StatusHead::Unavailable
                    | StatusHead::Detached
                    | StatusHead::Branch {
                        upstream: StatusUpstream::Missing,
                        ..
                    }
            )
        {
            return StatusClass::Warn;
        }
        if changes.is_dirty() || ahead(head) != CommitCount::default() {
            StatusClass::Pending
        } else {
            StatusClass::Clean
        }
    }

    #[must_use]
    pub fn detail(&self) -> String {
        let RepositoryStatus::Present { head, changes } = &self.repository else {
            return "not present".into();
        };
        let mut parts = Vec::new();
        match head {
            StatusHead::Unavailable => parts.push("branch-unavailable".to_string()),
            StatusHead::Detached => parts.push("detached".to_string()),
            StatusHead::Branch {
                upstream: StatusUpstream::Missing,
                ..
            } => parts.push("no-upstream".to_string()),
            StatusHead::Branch {
                upstream: StatusUpstream::Tracking { ahead, .. },
                ..
            } if *ahead != CommitCount::default() => parts.push(format!("⇡{ahead}")),
            StatusHead::Branch { .. } => {}
        }
        if let Some(detail) = changes_detail(changes) {
            parts.push(detail);
        }
        if parts.is_empty() {
            "✓".into()
        } else {
            parts.join(" ")
        }
    }
}

fn changes_detail(changes: &StatusChanges) -> Option<String> {
    match changes {
        StatusChanges::Changed { tracked, untracked } => Some(change_symbols(*tracked, *untracked)),
        StatusChanges::Unavailable => Some("status-unavailable".into()),
        StatusChanges::Clean => None,
    }
}

fn change_symbols(tracked: PathCount, untracked: PathCount) -> String {
    let mut symbols = String::new();
    if !tracked.is_zero() {
        symbols.push('!');
    }
    if !untracked.is_zero() {
        symbols.push('?');
    }
    symbols
}

impl Serialize for StatusResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (branch, upstream, ahead, changes) = presentation_facts(&self.repository);
        let (tracked, untracked) = changes.counts();
        let mut state = serializer.serialize_struct("StatusResult", 10)?;
        state.serialize_field("Name", &self.name)?;
        state.serialize_field("Present", &self.is_present())?;
        state.serialize_field("Branch", branch)?;
        state.serialize_field("Upstream", upstream)?;
        state.serialize_field("Ahead", &ahead.into_inner())?;
        state.serialize_field("Dirty", &changes.is_dirty())?;
        state.serialize_field("DirtyCount", &tracked.value())?;
        state.serialize_field("UntrackedCount", &untracked.value())?;
        state.serialize_field("State", self.class().as_str())?;
        state.serialize_field("Detail", &self.detail())?;
        state.end()
    }
}

fn ahead(head: &StatusHead) -> CommitCount {
    match head {
        StatusHead::Branch {
            upstream: StatusUpstream::Tracking { ahead, .. },
            ..
        } => *ahead,
        StatusHead::Unavailable
        | StatusHead::Detached
        | StatusHead::Branch {
            upstream: StatusUpstream::Missing,
            ..
        } => CommitCount::default(),
    }
}

fn presentation_facts(repository: &RepositoryStatus) -> (&str, &str, CommitCount, StatusChanges) {
    let RepositoryStatus::Present { head, changes } = repository else {
        return ("", "", CommitCount::default(), StatusChanges::Clean);
    };
    match head {
        StatusHead::Branch {
            name,
            upstream: StatusUpstream::Tracking { reference, ahead },
        } => (name.as_ref(), reference.as_ref(), *ahead, *changes),
        StatusHead::Branch {
            name,
            upstream: StatusUpstream::Missing,
        } => (name.as_ref(), "", CommitCount::default(), *changes),
        StatusHead::Detached => ("detached", "", CommitCount::default(), *changes),
        StatusHead::Unavailable => ("", "", CommitCount::default(), *changes),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn branch(raw: &str) -> BranchName {
        BranchName::try_new(raw).unwrap()
    }

    fn reference(raw: &str) -> GitRefName {
        GitRefName::try_new(raw).unwrap()
    }

    #[test]
    fn unavailable_status_is_a_warning_instead_of_a_false_clean_result() {
        let result = StatusResult::present(
            "api".try_into().unwrap(),
            StatusHead::Branch {
                name: branch("main"),
                upstream: StatusUpstream::Tracking {
                    reference: reference("origin/main"),
                    ahead: CommitCount::default(),
                },
            },
            StatusChanges::Unavailable,
        );

        assert_eq!(result.class(), StatusClass::Warn);
        assert_eq!(result.detail(), "status-unavailable");
    }

    #[test]
    fn legacy_json_fields_are_derived_from_closed_status_facts() {
        let result = StatusResult::present(
            "api".try_into().unwrap(),
            StatusHead::Branch {
                name: branch("main"),
                upstream: StatusUpstream::Tracking {
                    reference: reference("origin/main"),
                    ahead: CommitCount::new(2),
                },
            },
            StatusChanges::from_counts(PathCount::new(1), PathCount::new(1)),
        );

        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({
                "Name": "api",
                "Present": true,
                "Branch": "main",
                "Upstream": "origin/main",
                "Ahead": 2,
                "Dirty": true,
                "DirtyCount": 1,
                "UntrackedCount": 1,
                "State": "pending",
                "Detail": "⇡2 !?"
            })
        );
    }
}
