/// The structural role of one line in a Git unified diff.
///
/// # Examples
///
/// ```
/// use application::diffs::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
///
/// let mut classifier = UnifiedDiffLineClassifier::default();
/// assert_eq!(
///     classifier.classify("@@ -1 +1 @@"),
///     UnifiedDiffLineKind::Hunk {
///         line_number_old: 1,
///         line_number_new: 1,
///     }
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedDiffLineKind {
    /// Represents file headers, mode changes, rename markers, binary notices, or no-newline
    /// markers.
    Meta,
    /// Represents a valid hunk header and its absolute old/new starting line numbers.
    Hunk {
        line_number_old: u32,
        line_number_new: u32,
    },
    /// Represents a line present on both sides of the diff.
    Context,
    /// Represents a line present only on the new side.
    Added,
    /// Represents a line present only on the old side.
    Removed,
}

/// Classifies unified-diff lines for one file in source order.
///
/// # Examples
///
/// ```
/// use application::diffs::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
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
}

impl UnifiedDiffLineClassifier {
    /// Classifies `raw` and advances hunk state when it is a valid hunk header.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::diffs::{UnifiedDiffLineClassifier, UnifiedDiffLineKind};
    ///
    /// let mut classifier = UnifiedDiffLineClassifier::default();
    /// classifier.classify("@@ -4 +9 @@");
    /// assert_eq!(classifier.classify("+new"), UnifiedDiffLineKind::Added);
    /// ```
    pub fn classify(&mut self, raw: &str) -> UnifiedDiffLineKind {
        if let Some((line_number_old, line_number_new)) = hunk_line_numbers(raw) {
            self.hunk_started = true;
            return UnifiedDiffLineKind::Hunk {
                line_number_old,
                line_number_new,
            };
        }

        if raw.starts_with('\\') || (!self.hunk_started && is_file_metadata(raw)) {
            return UnifiedDiffLineKind::Meta;
        }

        match raw.as_bytes().first() {
            Some(b'+') => UnifiedDiffLineKind::Added,
            Some(b'-') => UnifiedDiffLineKind::Removed,
            _ => UnifiedDiffLineKind::Context,
        }
    }
}

fn is_file_metadata(raw: &str) -> bool {
    raw.starts_with("index ")
        || raw.starts_with("--- ")
        || raw.starts_with("+++ ")
        || raw.starts_with("new file")
        || raw.starts_with("deleted file")
        || raw.starts_with("old mode")
        || raw.starts_with("new mode")
        || raw.starts_with("similarity ")
        || raw.starts_with("rename ")
        || raw.starts_with("Binary ")
}

fn hunk_line_numbers(raw: &str) -> Option<(u32, u32)> {
    let rest = raw.strip_prefix("@@ -")?;
    let (range_old, rest) = rest.split_once(" +")?;
    let (range_new, _) = rest.split_once(" @@")?;
    Some((parse_range_start(range_old)?, parse_range_start(range_new)?))
}

fn parse_range_start(raw: &str) -> Option<u32> {
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

    start.parse().ok()
}
