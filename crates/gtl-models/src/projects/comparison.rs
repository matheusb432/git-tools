use nutype::nutype;

#[nutype(
    validate(predicate = valid_comparison_branch),
    default = "main".to_owned(),
    derive(Debug, Clone, Default, PartialEq, Eq, AsRef, Display, TryFrom, Serialize, Deserialize)
)]
pub struct ComparisonBranch(String);

fn valid_comparison_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !matches!(value, "HEAD" | "@")
        && !value.starts_with('-')
        && !value.starts_with("refs/")
        && !value.ends_with('.')
        && !value.contains(['\\', '~', '^', ':', '?', '*', '['])
        && !value.contains("..")
        && !value.contains("@{")
        && !value.bytes().any(|byte| byte <= b' ' || byte == 127)
        && value.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .rsplit_once('.')
                    .is_none_or(|(_, suffix)| suffix != "lock")
        })
}

impl ComparisonBranch {
    #[must_use]
    pub fn revision(&self) -> crate::git::GitRevision {
        crate::git::GitRevision::comparison_branch(self)
    }
}

#[cfg(test)]
mod tests {
    use super::ComparisonBranch;

    #[test]
    fn accepts_only_local_branch_names_at_construction_and_decoding() {
        for name in ["main", "develop", "release/next", "révision"] {
            assert!(ComparisonBranch::try_new(name).is_ok());
        }
        for name in [
            "",
            "HEAD",
            "@",
            "-main",
            "refs/heads/main",
            "main~1",
            "a..b",
            "a@{1}",
            "a b",
            "a//b",
            "a/.b",
            "a.lock/b",
            "a/",
            "a.",
            "a\\b",
        ] {
            assert!(ComparisonBranch::try_new(name).is_err(), "{name}");
            let encoded = serde_json::to_string(name).unwrap();
            assert!(serde_json::from_str::<ComparisonBranch>(&encoded).is_err());
        }
    }
}
