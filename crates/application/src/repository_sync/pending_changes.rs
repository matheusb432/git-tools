/// Summarizes the current repository changes shown before a push or commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingChanges {
    /// Counts distinct working-tree paths with any change.
    pub changed: usize,
    /// Counts paths with changes already staged in the index.
    pub staged: usize,
    /// Counts paths with unstaged or untracked changes.
    pub unprepared: usize,
    /// Counts commits ahead of the configured upstream.
    pub ahead: usize,
}

impl PendingChanges {
    /// Classifies porcelain-v1 status lines while retaining the supplied ahead count.
    pub(super) fn from_porcelain(porcelain: &str, ahead: usize) -> Self {
        let mut pending = Self {
            ahead,
            ..Self::default()
        };

        for line in porcelain.lines() {
            let bytes = line.as_bytes();
            if bytes.len() < 2 {
                continue;
            }

            pending.changed += 1;
            let (index, worktree) = (bytes[0], bytes[1]);
            if index == b'?' && worktree == b'?' {
                pending.unprepared += 1;
                continue;
            }
            if index != b' ' {
                pending.staged += 1;
            }
            if worktree != b' ' {
                pending.unprepared += 1;
            }
        }

        pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_classification_distinguishes_staged_and_unprepared_paths() {
        let porcelain = "M  a.txt\n M b.txt\nMM c.txt\n?? d.txt\n";

        let pending = PendingChanges::from_porcelain(porcelain, 2);

        assert_eq!(
            pending,
            PendingChanges {
                changed: 4,
                staged: 2,
                unprepared: 3,
                ahead: 2,
            }
        );
    }
}
