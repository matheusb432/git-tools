//! Pure classification of repository status output.

use domain::repository::PendingChanges;

/// Classifies porcelain-v1 status lines while retaining the supplied ahead count.
pub(super) fn classify(porcelain: &str, ahead: usize) -> PendingChanges {
    let mut pending = PendingChanges {
        ahead,
        ..PendingChanges::default()
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

#[cfg(test)]
mod tests {
    use domain::repository::PendingChanges;

    use super::classify;

    #[test]
    fn porcelain_classification_distinguishes_staged_and_unprepared_paths() {
        let porcelain = "M  a.txt\n M b.txt\nMM c.txt\n?? d.txt\n";

        let pending = classify(porcelain, 2);

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
