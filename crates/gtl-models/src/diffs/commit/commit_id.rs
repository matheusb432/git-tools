use nutype::nutype;

const COMMIT_ID_CHARACTER_COUNT: usize = 40;

/// Identifies a Git commit by its canonical 40-character SHA-1 object ID.
#[nutype(
    sanitize(with = normalize_commit_id),
    validate(with = validate_commit_id, error = CommitIdError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        AsRef,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct CommitId(String);

impl CommitId {
    /// Returns this commit ID at a supported presentation width.
    pub fn abbreviated(&self, abbreviation: CommitIdAbbreviation) -> String {
        self.as_ref()
            .chars()
            .take(abbreviation.character_count())
            .collect()
    }
}

/// Supported presentation widths for a Git commit ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommitIdAbbreviation {
    SevenCharacters,
    TenCharacters,
}

impl CommitIdAbbreviation {
    const fn character_count(self) -> usize {
        match self {
            Self::SevenCharacters => 7,
            Self::TenCharacters => 10,
        }
    }
}

/// Reports that raw input cannot represent a canonical Git commit SHA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("commit ID must contain exactly 40 ASCII hexadecimal characters")]
pub struct CommitIdError;

fn normalize_commit_id(mut value: String) -> String {
    value.make_ascii_lowercase();
    value
}

fn validate_commit_id(value: &str) -> Result<(), CommitIdError> {
    if value.len() == COMMIT_ID_CHARACTER_COUNT
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(CommitIdError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffs::PinnedRange;

    const COMMIT_ID_LOWERCASE: &str = "0123456789abcdef0123456789abcdef01234567";
    const COMMIT_ID_UPPERCASE: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

    fn commit_id(raw: &str) -> CommitId {
        raw.try_into().expect("fixture commit ID is valid")
    }

    #[test]
    fn try_into_validates_and_normalizes_owned_and_borrowed_git_shas() {
        let borrowed: Result<CommitId, _> = COMMIT_ID_UPPERCASE.try_into();
        let owned: Result<CommitId, _> = COMMIT_ID_UPPERCASE.to_owned().try_into();

        assert_eq!(
            borrowed.map(|id| id.to_string()),
            Ok(COMMIT_ID_LOWERCASE.to_owned())
        );
        assert_eq!(
            owned.map(|id| id.to_string()),
            Ok(COMMIT_ID_LOWERCASE.to_owned())
        );
        assert!(CommitId::try_from("0123456789abcdef").is_err());
        assert!(CommitId::try_from("0123456789abcdef0123456789abcdef0123456g").is_err());
    }

    #[test]
    fn abbreviates_to_the_closed_supported_widths() {
        let id = commit_id(COMMIT_ID_LOWERCASE);

        assert_eq!(
            id.abbreviated(CommitIdAbbreviation::SevenCharacters),
            "0123456"
        );
        assert_eq!(
            id.abbreviated(CommitIdAbbreviation::TenCharacters),
            "0123456789"
        );
    }

    #[test]
    fn serde_is_transparent_and_validated() {
        let id = commit_id(COMMIT_ID_LOWERCASE);
        assert_eq!(
            serde_json::to_string(&id).ok(),
            Some(format!("\"{COMMIT_ID_LOWERCASE}\""))
        );

        let uppercase_json = format!("\"{COMMIT_ID_UPPERCASE}\"");
        let parsed = serde_json::from_str::<CommitId>(&uppercase_json)
            .ok()
            .map(|id| id.to_string());
        assert_eq!(parsed, Some(COMMIT_ID_LOWERCASE.to_owned()));
        assert!(serde_json::from_str::<CommitId>("\"invalid\"").is_err());
    }

    #[test]
    fn pinned_range_retains_full_ids_and_derives_supported_presentations() {
        let range = PinnedRange {
            base: commit_id(COMMIT_ID_LOWERCASE),
            head: commit_id("abcdef0123456789abcdef0123456789abcdef01"),
        };

        assert_eq!(
            range.git_range(),
            "0123456789abcdef0123456789abcdef01234567..abcdef0123456789abcdef0123456789abcdef01"
        );
        assert_eq!(range.display_range(), "0123456789..abcdef0123");
        assert_eq!(range.display_base(), "0123456789");
        assert_eq!(
            serde_json::to_value(&range).ok(),
            Some(serde_json::json!({
                "base": COMMIT_ID_LOWERCASE,
                "head": "abcdef0123456789abcdef0123456789abcdef01"
            }))
        );
        assert!(
            serde_json::from_value::<PinnedRange>(serde_json::json!({
                "base": "short",
                "head": "abcdef0123456789abcdef0123456789abcdef01"
            }))
            .is_err()
        );
    }
}
