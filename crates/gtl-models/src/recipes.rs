mod label;

use std::{fmt, str::FromStr};

pub use label::{RecipeLabel, RecipeLabelChanges};
use uuid::Uuid;

/// Identifies one recipe batch with a validated UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RecipeBatchId(Uuid);

impl RecipeBatchId {
    /// Generates an opaque ID for a newly opened recipe batch.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for RecipeBatchId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for RecipeBatchId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::RecipeBatchId;

    const BATCH_ID: &str = "0198a859-7c4e-7e5f-9e63-ec7bb768d841";

    #[test]
    fn parsing_and_serde_reject_non_uuid_batch_ids() {
        let parsed: RecipeBatchId = BATCH_ID.parse().unwrap();

        assert_eq!(parsed.to_string(), BATCH_ID);
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            format!("\"{BATCH_ID}\"")
        );
        assert!("batch-1".parse::<RecipeBatchId>().is_err());
        assert!(serde_json::from_str::<RecipeBatchId>("\"batch-1\"").is_err());
    }

    #[test]
    fn generated_batch_ids_are_distinct() {
        assert_ne!(RecipeBatchId::generate(), RecipeBatchId::generate());
    }
}
