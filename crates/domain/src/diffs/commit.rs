/// One commit in range. `date`/`iso` are the human + machine timestamps.
/// `parents` are the short shas of its parents (≥2 ⇒ a merge); `members` are the
/// commits this merge brought into the previewed range (empty for a non-merge).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub iso: String,
    pub parents: Vec<String>,
    pub members: Vec<String>,
}

impl Commit {
    /// A commit with two or more parents is a merge.
    pub fn is_merge(&self) -> bool {
        self.parents.len() >= 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_merge_is_true_only_with_two_or_more_parents() {
        let mut commit = Commit::default();
        assert!(!commit.is_merge()); // 0 parents (root)
        commit.parents = vec!["p1aaaaaaa".to_string()];
        assert!(!commit.is_merge()); // 1 parent (normal)
        commit.parents.push("p2bbbbbbb".to_string());
        assert!(commit.is_merge()); // 2 parents (merge)
    }
}
