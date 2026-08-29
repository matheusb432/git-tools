#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteLevel {
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub level: NoteLevel,
    pub text: String,
}

impl Note {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: NoteLevel::Info,
            text: text.into(),
        }
    }

    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            level: NoteLevel::Warn,
            text: text.into(),
        }
    }
}
