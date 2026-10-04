use std::fmt;

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};

/// Why a diff between two revisions of a hosted repository cannot be fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteDiffFailure {
    /// The origin does not name a repository on github.com.
    UnsupportedOrigin,
    /// The range is not `BASE...HEAD` with two revisions GitHub can compare.
    InvalidRange,
    /// The GitHub CLI has no usable credentials.
    Unauthenticated,
    /// GitHub has no such repository or revisions visible to the credentials.
    NotFound,
    /// GitHub's API rate limit is exhausted.
    RateLimited,
    /// GitHub refused or failed the comparison with an HTTP `status`.
    Rejected {
        status: u16,
        diagnostic: ExternalDiagnostic,
    },
}

impl RemoteDiffFailure {
    /// GitHub's explanation of a refused comparison.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::Rejected { diagnostic, .. } => Some(diagnostic),
            _ => None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::UnsupportedOrigin | Self::InvalidRange => ErrorClass::InvalidArgument,
            Self::Unauthenticated => ErrorClass::Unauthenticated,
            Self::NotFound => ErrorClass::NotFound,
            Self::RateLimited => ErrorClass::ResourceExhausted,
            Self::Rejected { status, .. } if *status >= 500 => ErrorClass::Unavailable,
            Self::Rejected { .. } => ErrorClass::FailedPrecondition,
        }
    }
}

impl PublicFailure for RemoteDiffFailure {
    fn failure(&self) -> Failure {
        Failure::RemoteDiff(self.clone())
    }
}

impl From<RemoteDiffFailure> for Failure {
    fn from(failure: RemoteDiffFailure) -> Self {
        Self::RemoteDiff(failure)
    }
}

impl std::error::Error for RemoteDiffFailure {}

impl fmt::Display for RemoteDiffFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOrigin => formatter.write_str(
                "The origin must be an HTTPS or SSH URL of a github.com repository.",
            ),
            Self::InvalidRange => formatter.write_str(
                "The range must be `BASE...HEAD`; GitHub compares HEAD with the merge base of both revisions.",
            ),
            Self::Unauthenticated => formatter
                .write_str("The GitHub CLI is not signed in. Run `gh auth login` and try again."),
            Self::NotFound => formatter.write_str(
                "GitHub found no such repository or revisions for the signed-in account.",
            ),
            Self::RateLimited => {
                formatter.write_str("GitHub's API rate limit is exhausted. Try again later.")
            }
            Self::Rejected { status, .. } => {
                write!(formatter, "GitHub refused the comparison (HTTP {status}).")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_comparisons_are_unavailable_only_for_server_errors() {
        let rejected = |status| RemoteDiffFailure::Rejected {
            status,
            diagnostic: ExternalDiagnostic::new("Sorry, this diff is unavailable."),
        };

        assert_eq!(rejected(406).class(), ErrorClass::FailedPrecondition);
        assert_eq!(rejected(502).class(), ErrorClass::Unavailable);
        assert_eq!(
            rejected(406).diagnostic().map(ExternalDiagnostic::as_str),
            Some("Sorry, this diff is unavailable.")
        );
    }
}
