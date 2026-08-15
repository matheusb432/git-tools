use similar::{ChangeTag, TextDiff};

/// A half-open character range within a changed line body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterSpan {
    start: usize,
    end: usize,
}

impl CharacterSpan {
    pub(crate) const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the first character index covered by the span.
    pub const fn start(&self) -> usize {
        self.start
    }

    /// Returns the exclusive character index after the span.
    pub const fn end(&self) -> usize {
        self.end
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ChangedLineSpans {
    pub(crate) old: Vec<CharacterSpan>,
    pub(crate) new: Vec<CharacterSpan>,
}

pub(crate) fn changed_spans(old: &str, new: &str) -> ChangedLineSpans {
    let diff = TextDiff::from_chars(old, new);
    let mut spans = ChangedLineSpans::default();
    let mut old_idx = 0usize;
    let mut new_idx = 0usize;
    let mut saw_common = false;

    for change in diff.iter_all_changes() {
        let len = change.value().chars().count();
        match change.tag() {
            ChangeTag::Equal => {
                saw_common = true;
                old_idx += len;
                new_idx += len;
            }
            ChangeTag::Delete => {
                push_span(&mut spans.old, old_idx, old_idx + len);
                old_idx += len;
            }
            ChangeTag::Insert => {
                push_span(&mut spans.new, new_idx, new_idx + len);
                new_idx += len;
            }
        }
    }

    if saw_common {
        spans
    } else {
        ChangedLineSpans::default()
    }
}

fn push_span(spans: &mut Vec<CharacterSpan>, start: usize, end: usize) {
    match spans.last_mut() {
        Some(last) if last.end == start => last.end = end,
        _ => spans.push(CharacterSpan::new(start, end)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize) -> CharacterSpan {
        CharacterSpan::new(start, end)
    }

    #[test]
    fn marks_only_the_single_differing_run() {
        let spans = changed_spans("let x = 1;", "let x = 2;");
        assert_eq!(spans.old, vec![span(8, 9)]);
        assert_eq!(spans.new, vec![span(8, 9)]);
    }

    #[test]
    fn merges_contiguous_inserted_chars_into_one_span() {
        let spans = changed_spans("foobar", "fooXYbar");
        assert!(spans.old.is_empty());
        assert_eq!(spans.new, vec![span(3, 5)]);
    }

    #[test]
    fn reports_multiple_separated_changed_runs() {
        let spans = changed_spans("a1b2c", "aXbYc");
        assert_eq!(spans.old, vec![span(1, 2), span(3, 4)]);
        assert_eq!(spans.new, vec![span(1, 2), span(3, 4)]);
    }

    #[test]
    fn suppresses_spans_when_lines_share_no_common_run() {
        assert_eq!(changed_spans("old", "new"), ChangedLineSpans::default());
    }
}
