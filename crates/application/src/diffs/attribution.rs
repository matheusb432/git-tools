//! Per-line commit attribution for the diff preview.
//!
//! Each aggregate-diff row is mapped to the commit that owns it: forward
//! `git blame` for added rows (new-side line → author), reverse `git blame` for
//! deleted rows (old-side line → the `previous` commit that removed it). Only
//! shas inside the previewed range are kept, so an unattributable row is left
//! bare rather than mis-assigned.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use super::FileDiff;
use crate::ports::GitClient;

/// The new (right) side of the previewed diff: a committed tip, or the working
/// tree (hash mode, where the diff is `base` → working tree).
pub enum NewSide {
    Commit(String),
    WorkTree,
}

/// Fill each file's `owners` from git blame, keeping only in-range shas. Blame
/// errors are swallowed per file (it goes unattributed) so one awkward file —
/// deleted, binary, renamed — never aborts the preview.
pub fn attribute<S: std::hash::BuildHasher>(
    source: &impl GitClient,
    repo: &Path,
    base: &str,
    new_side: &NewSide,
    in_range: &HashSet<String, S>,
    files: &mut [FileDiff],
) {
    let tip = match new_side {
        NewSide::Commit(tip) => tip.as_str(),
        NewSide::WorkTree => "HEAD",
    };
    for file in files.iter_mut() {
        let forward = match new_side {
            NewSide::Commit(tip) => source.blame_forward(repo, base, tip, &file.path),
            NewSide::WorkTree => source.blame_forward_worktree(repo, &file.path),
        };
        if let Ok(lines) = forward {
            file.owners.added = retain_range(lines, in_range);
        }
        if let Ok(lines) = source.blame_reverse(repo, base, tip, &file.path) {
            file.owners.deleted = retain_range(lines, in_range);
        }
    }
}

fn retain_range<S: std::hash::BuildHasher>(
    mut lines: HashMap<u32, String>,
    in_range: &HashSet<String, S>,
) -> HashMap<u32, String> {
    lines.retain(|_, sha| in_range.contains(sha));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(shas: &[&str]) -> HashSet<String> {
        shas.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn retain_range_keeps_only_attributions_from_the_previewed_range() {
        let lines = HashMap::from([
            (3, "6ea844233".to_string()),
            (5, "9ccfcca0b".to_string()),
            (1, "93b22953a".to_string()),
        ]);
        let m = retain_range(lines, &set(&["6ea844233", "9ccfcca0b"]));
        assert_eq!(m.get(&3), Some(&"6ea844233".to_string()));
        assert_eq!(m.get(&5), Some(&"9ccfcca0b".to_string()));
        assert_eq!(m.get(&1), None);
    }
}
