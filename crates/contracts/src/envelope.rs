//! The common response envelope: a typed outcome, service-owned user-facing
//! note lines (printed verbatim by clients), and optional per-feature data.

use serde::{Deserialize, Serialize};

/// The typed, machine-readable result of a daemon request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Empty,
    Error,
}

/// The severity of a [`Note`], which determines which stream a client routes it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteLevel {
    Info,
    Warn,
    Error,
}

/// One service-owned, user-facing output line, printed verbatim by clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub level: NoteLevel,
    pub text: String,
}

/// The common response envelope: an outcome, its notes, and optional per-feature data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub outcome: Outcome,
    pub notes: Vec<Note>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

fn none<T>() -> Option<T> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outcome_wire_format() {
        assert_eq!(serde_json::to_string(&Outcome::Ok).unwrap(), "\"ok\"");
        assert_eq!(serde_json::to_string(&Outcome::Empty).unwrap(), "\"empty\"");
        assert_eq!(serde_json::to_string(&Outcome::Error).unwrap(), "\"error\"");
    }

    #[test]
    fn test_note_level_wire_format() {
        assert_eq!(serde_json::to_string(&NoteLevel::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&NoteLevel::Warn).unwrap(), "\"warn\"");
        assert_eq!(
            serde_json::to_string(&NoteLevel::Error).unwrap(),
            "\"error\""
        );
    }

    #[test]
    fn test_envelope_roundtrip_without_data() {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct TestData {
            value: String,
        }

        let original = Envelope {
            outcome: Outcome::Error,
            notes: vec![Note {
                level: NoteLevel::Error,
                text: "Something went wrong".to_string(),
            }],
            data: None::<TestData>,
        };

        let json = serde_json::to_string(&original).unwrap();
        // Assert that the JSON does not contain the "data" field
        assert!(!json.contains("\"data\""));

        let deserialized: Envelope<TestData> = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
