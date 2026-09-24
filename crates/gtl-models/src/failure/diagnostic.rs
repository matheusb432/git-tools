use std::fmt;

use serde::{Deserialize, Serialize};

/// Verbatim output from an external tool, such as Git stderr, bounded to
/// [`ExternalDiagnostic::CHARACTERS_MAX`] characters.
///
/// Presentation shows it as secondary detail and never translates it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct ExternalDiagnostic(String);

impl ExternalDiagnostic {
    pub const CHARACTERS_MAX: usize = 4096;

    /// Trims surrounding whitespace and truncates the text to the character bound.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self(text.trim().chars().take(Self::CHARACTERS_MAX).collect())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for ExternalDiagnostic {
    fn from(text: String) -> Self {
        Self::new(&text)
    }
}

impl From<ExternalDiagnostic> for String {
    fn from(diagnostic: ExternalDiagnostic) -> Self {
        diagnostic.0
    }
}

impl fmt::Display for ExternalDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::ExternalDiagnostic;

    #[test]
    fn diagnostics_are_trimmed_and_bounded_on_every_construction_path() {
        let long = "x".repeat(ExternalDiagnostic::CHARACTERS_MAX + 10);

        assert_eq!(
            ExternalDiagnostic::new("  fatal: gone \n").as_str(),
            "fatal: gone"
        );
        assert_eq!(
            ExternalDiagnostic::new(&long).as_str().chars().count(),
            ExternalDiagnostic::CHARACTERS_MAX
        );
        let decoded: ExternalDiagnostic = serde_json::from_str(&format!("\"{long}\"")).unwrap();
        assert_eq!(
            decoded.as_str().chars().count(),
            ExternalDiagnostic::CHARACTERS_MAX
        );
    }
}
