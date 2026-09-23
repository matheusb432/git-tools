use std::{fmt, str::FromStr};

use uuid::Uuid;

/// Identifies a prepared push within one server's bounded operation history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ViewerPushId(Uuid);

impl ViewerPushId {
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for ViewerPushId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ViewerPushId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}
