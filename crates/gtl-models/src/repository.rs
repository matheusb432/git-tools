//! Structured values describing one repository's local state.

pub mod recursive_push;
pub mod status;
pub mod traversal;
pub mod working_tree;

use std::fmt;

use crate::git::CommitCount;

/// Counts repository-relative paths in a working-tree state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathCount(u64);

impl PathCount {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn from_len(value: usize) -> Self {
        Self(path_count_from_usize(value))
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn increment(&mut self) {
        *self = Self(incremented_path_count(self.0));
    }
}

impl fmt::Display for PathCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[allow(clippy::unreachable)]
fn path_count_from_usize(value: usize) -> u64 {
    match u64::try_from(value) {
        Ok(value) => value,
        Err(_) => unreachable!("supported targets use at most 64-bit collection lengths"),
    }
}

#[allow(clippy::unreachable)]
fn incremented_path_count(value: u64) -> u64 {
    match value.checked_add(1) {
        Some(value) => value,
        None => unreachable!("a repository cannot contain more than u64::MAX paths"),
    }
}

/// Summarizes repository changes shown before a push or commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingChanges {
    /// Counts distinct working-tree paths with any change.
    pub changed: PathCount,
    /// Counts paths with changes already staged in the index.
    pub staged: PathCount,
    /// Counts paths with unstaged or untracked changes.
    pub unprepared: PathCount,
    /// Counts commits ahead of the configured upstream.
    pub ahead: CommitCount,
}
