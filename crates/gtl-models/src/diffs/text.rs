use std::fmt::{self, Write as _};

use nutype::nutype;
use sha2::{Digest as _, Sha256};

/// The largest diff text accepted from outside a repository.
pub const DIFF_TEXT_BYTES_MAX: usize = 64 * 1024 * 1024;

const DIFF_TEXT_ID_CHARACTER_COUNT: usize = 64;

/// Identifies diff text by the lowercase hexadecimal SHA-256 digest of its bytes.
#[nutype(
    sanitize(with = normalize_diff_text_id),
    validate(with = validate_diff_text_id, error = DiffTextIdError),
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
pub struct DiffTextId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("diff text ID must contain exactly 64 ASCII hexadecimal characters")]
pub struct DiffTextIdError;

fn normalize_diff_text_id(mut value: String) -> String {
    value.make_ascii_lowercase();
    value
}

fn validate_diff_text_id(value: &str) -> Result<(), DiffTextIdError> {
    if value.len() == DIFF_TEXT_ID_CHARACTER_COUNT
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(DiffTextIdError)
    }
}

/// Unparsed diff text within [`DIFF_TEXT_BYTES_MAX`], paired with its content identity.
#[derive(Clone, PartialEq, Eq)]
pub struct DiffText {
    id: DiffTextId,
    text: Box<str>,
}

impl DiffText {
    /// Accepts text up to [`DIFF_TEXT_BYTES_MAX`] bytes.
    pub fn try_new(text: String) -> Result<Self, DiffTextError> {
        if text.len() > DIFF_TEXT_BYTES_MAX {
            return Err(DiffTextError::TooLarge);
        }
        let mut encoded = String::with_capacity(DIFF_TEXT_ID_CHARACTER_COUNT);
        for byte in Sha256::digest(text.as_bytes()) {
            let _ = write!(encoded, "{byte:02x}");
        }
        Ok(Self {
            id: known_valid(DiffTextId::try_new(encoded)),
            text: text.into_boxed_str(),
        })
    }

    #[must_use]
    pub fn id(&self) -> &DiffTextId {
        &self.id
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl fmt::Debug for DiffText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiffText")
            .field("id", &self.id)
            .field("byte_count", &self.text.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DiffTextError {
    #[error("diff text exceeds {DIFF_TEXT_BYTES_MAX} bytes")]
    TooLarge,
}

#[allow(clippy::unreachable)]
fn known_valid(result: Result<DiffTextId, DiffTextIdError>) -> DiffTextId {
    match result {
        Ok(id) => id,
        Err(DiffTextIdError) => unreachable!("a SHA-256 digest always encodes a valid identity"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_the_sha256_of_the_text() {
        let text = DiffText::try_new("abc".to_owned()).unwrap();

        assert_eq!(
            text.id().as_ref(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(text.as_str(), "abc");
    }

    #[test]
    fn text_over_the_byte_limit_is_rejected() {
        assert_eq!(
            DiffText::try_new("x".repeat(DIFF_TEXT_BYTES_MAX + 1)),
            Err(DiffTextError::TooLarge)
        );
        assert!(DiffText::try_new("x".repeat(DIFF_TEXT_BYTES_MAX)).is_ok());
    }

    #[test]
    fn identity_parsing_normalizes_case_and_rejects_other_lengths() {
        let upper = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";

        assert_eq!(
            DiffTextId::try_new(upper.to_owned()).unwrap().as_ref(),
            upper.to_ascii_lowercase()
        );
        assert_eq!(
            DiffTextId::try_new("ba7816bf".to_owned()),
            Err(DiffTextIdError)
        );
    }
}
