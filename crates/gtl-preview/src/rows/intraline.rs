//! Computes intra-line changed spans for the renderer's side-by-side view.
//!
//! Given a paired deletion/addition line, [`changed_spans`] computes the changed
//! character runs on each side so the renderer can highlight only what actually
//! differs within an otherwise line-highlighted row — the VS Code "inline diff" look.

use similar::{ChangeTag, TextDiff};

pub(super) const PRESENTATION_CLASSES: &str = concat!(
    "[&_.diff-split_:is(.sp-del,.sp-add)_.ciw]:rounded-[2px] ",
    "[&_.diff-split_.sp-del_.ciw]:bg-[color-mix(in_srgb,var(--del)_34%,transparent)] ",
    "[&_.diff-split_.sp-add_.ciw]:bg-[color-mix(in_srgb,var(--add)_34%,transparent)]",
);

/// A half-open `[start, end)` range of char indices within a line body marking a
/// changed run. Indices count `char`s, matching the renderer's per-`char` walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Changed character spans within a paired deletion/addition line: `old` indexes the
/// deletion body, `new` the addition body. Empty (the default) when the two bodies
/// share no common run — the whole line already reads as changed via its line color,
/// so an intra-line highlight would be redundant noise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LineSpans {
    pub(super) old: Vec<Span>,
    pub(super) new: Vec<Span>,
}

/// Char-level diff of two changed-line bodies (markers already stripped), returning the
/// changed char spans per side with contiguous runs merged. Returns empty spans when the
/// bodies share no common character run.
pub(super) fn changed_spans(old: &str, new: &str) -> LineSpans {
    let diff = TextDiff::from_chars(old, new);
    let mut spans = LineSpans::default();
    // TODO: why usize? could use u32, instead.
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

    // ! No common run means the line was rewritten wholesale; the line color already says
    // ! that, so emitting a span over the entire body would just be redundant noise.
    if saw_common {
        spans
    } else {
        LineSpans::default()
    }
}

/// Append `[start, end)`, extending the previous span instead when it abuts it so a run of
/// adjacent single-char changes collapses into one highlight.
fn push_span(spans: &mut Vec<Span>, start: usize, end: usize) {
    match spans.last_mut() {
        Some(last) if last.end == start => last.end = end,
        _ => spans.push(Span { start, end }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize) -> Span {
        Span { start, end }
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
        assert!(
            spans.old.is_empty(),
            "pure insertion leaves the old side unmarked"
        );
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
        assert_eq!(changed_spans("old", "new"), LineSpans::default());
    }
}
