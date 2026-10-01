//! Read-only installation and startup diagnostics shared by native processes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
    pub action: Option<String>,
}

impl Check {
    pub fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
            action: None,
        }
    }
    pub fn warning(
        name: impl Into<String>,
        detail: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warning,
            detail: detail.into(),
            action: Some(action.into()),
        }
    }
    pub fn fail(
        name: impl Into<String>,
        detail: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
            action: Some(action.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DoctorReport {
    pub version: String,
    pub checks: Vec<Check>,
}

impl DoctorReport {
    #[must_use]
    pub fn failed(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == CheckStatus::Fail)
    }
}
