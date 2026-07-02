//! User-facing messages a slice wants surfaced, carried out of the core as data
//! so the caller (cli today, daemon client tomorrow) decides how to render them.

/// Where a [`Note`] should land when a terminal caller prints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteLevel {
    /// Informational — goes to stdout.
    Info,
    /// Warning — goes to stderr.
    Warn,
}

/// One human-facing message emitted by a slice, tagged with its [`NoteLevel`].
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub level: NoteLevel,
    pub text: String,
}

impl Note {
    /// An informational note (stdout).
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: NoteLevel::Info,
            text: text.into(),
        }
    }

    /// A warning note (stderr).
    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            level: NoteLevel::Warn,
            text: text.into(),
        }
    }
}
