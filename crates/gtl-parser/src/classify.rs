use crate::SourceLineNumber;

/// The structural role of one line in a Git unified diff.
///
/// # Examples
///
/// ```
/// use gtl_parser::{SourceLineNumber, UnifiedDiffLineClassifier, UnifiedDiffLineKind};
///
/// let mut classifier = UnifiedDiffLineClassifier::default();
/// assert_eq!(
///     classifier.classify("@@ -1 +1 @@"),
///     UnifiedDiffLineKind::Hunk {
///         line_number_old: SourceLineNumber::new(1),
///         line_number_new: SourceLineNumber::new(1),
///     }
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedDiffLineKind {
    /// A file header, mode change, rename or copy marker, binary notice or patch, or no-newline
    /// marker.
    Meta,
    /// A valid hunk header and its absolute old and new starting line numbers.
    Hunk {
        line_number_old: SourceLineNumber,
        line_number_new: SourceLineNumber,
    },
    /// A line present on both sides of the diff.
    Context,
    /// A line present only on the new side.
    Added,
    /// A line present only on the old side.
    Removed,
}

/// Classifies unified-diff lines for one file in source order.
///
/// A classifier instance must not be reused across files because file headers
/// are distinguished from header-like hunk content through accumulated state.
///
/// # Examples
///
/// ```
/// use gtl_parser::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
///
/// let mut classifier = UnifiedDiffLineClassifier::default();
/// assert_eq!(classifier.classify("--- a/file"), UnifiedDiffLineKind::Meta);
/// classifier.classify("@@ -1 +1 @@");
/// assert_eq!(
///     classifier.classify("--- heading"),
///     UnifiedDiffLineKind::Removed
/// );
/// ```
#[derive(Debug, Default)]
pub struct UnifiedDiffLineClassifier {
    hunk_started: bool,
    binary_patch_started: bool,
}

impl UnifiedDiffLineClassifier {
    /// Classifies `raw` and advances hunk state when it is a valid hunk header.
    pub fn classify(&mut self, raw: &str) -> UnifiedDiffLineKind {
        if self.binary_patch_started {
            return UnifiedDiffLineKind::Meta;
        }
        if let Some((line_number_old, line_number_new)) = hunk_line_numbers(raw) {
            self.hunk_started = true;
            return UnifiedDiffLineKind::Hunk {
                line_number_old,
                line_number_new,
            };
        }

        if raw.starts_with('\\') || (!self.hunk_started && is_file_metadata(raw)) {
            // Encoded binary data follows the marker until the file ends.
            self.binary_patch_started = raw == BINARY_PATCH_MARKER;
            return UnifiedDiffLineKind::Meta;
        }

        match raw.as_bytes().first() {
            Some(b'+') => UnifiedDiffLineKind::Added,
            Some(b'-') => UnifiedDiffLineKind::Removed,
            _ => UnifiedDiffLineKind::Context,
        }
    }
}

const BINARY_PATCH_MARKER: &str = "GIT binary patch";

fn is_file_metadata(raw: &str) -> bool {
    raw.starts_with("index ")
        || raw.starts_with("--- ")
        || raw.starts_with("+++ ")
        || raw.starts_with("new file")
        || raw.starts_with("deleted file")
        || raw.starts_with("old mode")
        || raw.starts_with("new mode")
        || raw.starts_with("similarity ")
        || raw.starts_with("dissimilarity ")
        || raw.starts_with("rename ")
        || raw.starts_with("copy from ")
        || raw.starts_with("copy to ")
        || raw.starts_with("Binary ")
        || raw == BINARY_PATCH_MARKER
}

fn hunk_line_numbers(raw: &str) -> Option<(SourceLineNumber, SourceLineNumber)> {
    let rest = raw.strip_prefix("@@ -")?;
    let (range_old, rest) = rest.split_once(" +")?;
    let (range_new, _) = rest.split_once(" @@")?;
    Some((parse_range_start(range_old)?, parse_range_start(range_new)?))
}

fn parse_range_start(raw: &str) -> Option<SourceLineNumber> {
    let (start, length) = match raw.split_once(',') {
        Some((start, length)) => (start, Some(length)),
        None => (raw, None),
    };

    if start.is_empty()
        || !start.chars().all(|character| character.is_ascii_digit())
        || length.is_some_and(|value| {
            value.is_empty() || !value.chars().all(|character| character.is_ascii_digit())
        })
    {
        return None;
    }

    start.parse::<u32>().ok().map(SourceLineNumber::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_headers_stop_being_metadata_inside_a_hunk() {
        let mut classifier = UnifiedDiffLineClassifier::default();

        assert_eq!(classifier.classify("--- a/file"), UnifiedDiffLineKind::Meta);
        classifier.classify("@@ -1 +1 @@");
        assert_eq!(
            classifier.classify("+++ literal"),
            UnifiedDiffLineKind::Added
        );
        assert_eq!(
            classifier.classify("--- heading"),
            UnifiedDiffLineKind::Removed
        );
    }

    #[test]
    fn copy_and_dissimilarity_headers_are_metadata() {
        let mut classifier = UnifiedDiffLineClassifier::default();

        for header in [
            "dissimilarity index 90%",
            "copy from src/old.rs",
            "copy to src/new.rs",
        ] {
            assert_eq!(classifier.classify(header), UnifiedDiffLineKind::Meta);
        }
    }

    #[test]
    fn binary_patch_data_is_metadata() {
        let mut classifier = UnifiedDiffLineClassifier::default();

        for line in [
            "index 1111111..2222222 100644",
            "GIT binary patch",
            "literal 12",
            "zcmZ?wbhEHbRA2y$+5oEnC",
            "",
            "@@ -1 +1 @@",
            "+looks added",
        ] {
            assert_eq!(classifier.classify(line), UnifiedDiffLineKind::Meta);
        }
    }

    #[test]
    fn malformed_hunk_header_is_context() {
        assert_eq!(
            UnifiedDiffLineClassifier::default().classify("@@ garbage @@"),
            UnifiedDiffLineKind::Context
        );
    }
}
