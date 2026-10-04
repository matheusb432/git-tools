//! Complete terminal snapshots, independent of the desktop tab lifecycle.

use std::collections::HashSet;

use crate::viewer::ViewerCodeSpan;

pub const SNAPSHOT_BYTES_MAX: usize = 64 * 1024 * 1024;
pub const SNAPSHOT_ROWS_MAX: usize = 500_000;
pub const SNAPSHOT_FILES_MAX: usize = 8192;
pub const ROW_BYTES_MAX: usize = 256 * 1024;
pub const BATCH_ROWS_MAX: usize = 128;
pub const MESSAGE_BYTES_MAX: usize = 512 * 1024;
pub const ROW_SYNTAX_SPANS_MAX: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Meta,
    Hunk,
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub text: String,
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
    pub syntax: Vec<ViewerCodeSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub review: Option<crate::diff_review::DiffFileReview>,
    pub path: String,
    pub added: u64,
    pub removed: u64,
    pub compact: Vec<Row>,
    pub full: Vec<Row>,
}

impl File {
    #[must_use]
    pub fn rows(&self, full: bool) -> &[Row] {
        if full && !self.full.is_empty() {
            &self.full
        } else {
            &self.compact
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalDiff {
    title: String,
    notes: Vec<String>,
    files: Vec<File>,
}

impl TerminalDiff {
    pub fn try_new(
        title: String,
        notes: Vec<String>,
        files: Vec<File>,
    ) -> Result<Self, SnapshotError> {
        let mut budget = SnapshotBudget::default();
        budget.add_header(&title, &notes)?;
        let mut paths = HashSet::new();
        for file in &files {
            budget.add_file(&file.path)?;
            budget.add_review(file.review.as_ref())?;
            if file
                .review
                .as_ref()
                .is_some_and(|review| review.reference.path.to_str() != Some(file.path.as_str()))
            {
                return Err(SnapshotError::Invalid);
            }
            if !paths.insert(&file.path) {
                return Err(SnapshotError::Invalid);
            }
            for row in file.compact.iter().chain(&file.full) {
                budget.add_row(row)?;
            }
        }
        Ok(Self {
            title,
            notes,
            files,
        })
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }
    #[must_use]
    pub fn files(&self) -> &[File] {
        &self.files
    }

    #[must_use]
    pub fn into_parts(self) -> (String, Vec<String>, Vec<File>) {
        (self.title, self.notes, self.files)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    Invalid,
    TooLarge,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid or incomplete terminal diff snapshot",
            Self::TooLarge => {
                "terminal diff exceeds the snapshot limit; select a smaller revision range"
            }
        })
    }
}
impl std::error::Error for SnapshotError {}

#[derive(Default)]
pub struct SnapshotBudget {
    bytes: usize,
    rows: usize,
    files: usize,
}

impl SnapshotBudget {
    pub fn add_review(
        &mut self,
        review: Option<&crate::diff_review::DiffFileReview>,
    ) -> Result<(), SnapshotError> {
        if let Some(review) = review {
            if let gtl_models::diffs::DiffReviewScope::Repository(repository) =
                &review.reference.scope
            {
                self.add_text(&repository.to_string_lossy())?;
            }
            self.add_text(&review.reference.path.to_string_lossy())?;
            self.bytes = self
                .bytes
                .saturating_add(std::mem::size_of::<crate::diff_review::DiffFileReview>());
            if self.bytes > SNAPSHOT_BYTES_MAX {
                return Err(SnapshotError::TooLarge);
            }
        }
        Ok(())
    }
    pub fn add_header(&mut self, title: &str, notes: &[String]) -> Result<(), SnapshotError> {
        if notes.len() > SNAPSHOT_FILES_MAX {
            return Err(SnapshotError::TooLarge);
        }
        self.add_text(title)?;
        for note in notes {
            self.add_text(note)?;
        }
        Ok(())
    }
    pub fn add_file(&mut self, path: &str) -> Result<(), SnapshotError> {
        if path.is_empty() {
            return Err(SnapshotError::Invalid);
        }
        self.files += 1;
        if self.files > SNAPSHOT_FILES_MAX {
            return Err(SnapshotError::TooLarge);
        }
        self.add_text(path)
    }
    pub fn add_row(&mut self, row: &Row) -> Result<(), SnapshotError> {
        if row.syntax.len() > ROW_SYNTAX_SPANS_MAX {
            return Err(SnapshotError::TooLarge);
        }
        let mut end = 0;
        for span in &row.syntax {
            if span.byte_start != end
                || span.byte_end <= span.byte_start
                || row.text.get(span.byte_start..span.byte_end).is_none()
            {
                return Err(SnapshotError::Invalid);
            }
            end = span.byte_end;
        }
        if !row.syntax.is_empty() && end != row.text.len() {
            return Err(SnapshotError::Invalid);
        }
        self.rows += 1;
        self.bytes = self.bytes.saturating_add(
            std::mem::size_of::<Row>() + row.syntax.len() * std::mem::size_of::<ViewerCodeSpan>(),
        );
        if self.rows > SNAPSHOT_ROWS_MAX {
            return Err(SnapshotError::TooLarge);
        }
        self.add_text(&row.text)
    }
    fn add_text(&mut self, text: &str) -> Result<(), SnapshotError> {
        self.bytes = self.bytes.saturating_add(text.len());
        if text.len() > ROW_BYTES_MAX || self.bytes > SNAPSHOT_BYTES_MAX {
            return Err(SnapshotError::TooLarge);
        }
        Ok(())
    }
}
