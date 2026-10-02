use std::ops::Range;

use gtl_wire::terminal_diff::{Row, RowKind};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use super::{Pager, theme};

pub(super) fn draw(frame: &mut Frame<'_>, pager: &Pager, area: Rect) {
    for screen in 0..area.height {
        let offset = pager.offset + usize::from(screen);
        if offset >= pager.layout.screen_rows {
            break;
        }
        let Some(entry) = pager.layout.entry_at(offset) else {
            break;
        };
        let source = pager.layout.source(&pager.document, entry);
        let text = pager.layout.text(&pager.document, entry);
        let style = source.map_or_else(
            || theme::surface(theme::SURFACE).add_modifier(Modifier::BOLD),
            |row| theme::row(row.kind),
        );
        let mut spans = source.map_or_else(Vec::new, |row| {
            gutter(
                row,
                offset > entry.screen_start,
                area.width,
                pager.layout.gutter,
            )
        });
        let gutter_width = if source.is_some() {
            pager.layout.gutter
        } else {
            0
        };
        let (range, padding) = if pager.wrap {
            (
                pager.layout.visible_range(&pager.document, entry, offset),
                0,
            )
        } else {
            horizontal_range(
                text,
                pager.horizontal,
                usize::from(area.width).saturating_sub(gutter_width),
            )
        };
        if padding > 0 {
            spans.push(Span::raw(" ".repeat(padding)));
        }
        let mut code = styled_spans(text, source, range);
        if pager.search.matches(entry.screen_start) {
            for span in &mut code {
                span.style = span
                    .style
                    .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
            }
        }
        spans.extend(code);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(style),
            Rect::new(area.x, area.y + screen, area.width, 1),
        );
    }
}

fn styled_spans<'a>(text: &'a str, source: Option<&Row>, range: Range<usize>) -> Vec<Span<'a>> {
    let Some(row) = source.filter(|row| !row.syntax.is_empty()) else {
        return vec![Span::raw(&text[range])];
    };
    let first = row
        .syntax
        .partition_point(|span| span.byte_end <= range.start);
    row.syntax[first..]
        .iter()
        .take_while(|span| span.byte_start < range.end)
        .filter_map(|span| {
            let start = span.byte_start.max(range.start);
            let end = span.byte_end.min(range.end);
            (start < end).then(|| {
                Span::styled(
                    &text[start..end],
                    span.syntax_class
                        .map_or_else(Default::default, theme::syntax),
                )
            })
        })
        .collect()
}

fn gutter(row: &Row, continuation: bool, width: u16, gutter: usize) -> Vec<Span<'static>> {
    let old = row
        .old_line_number
        .map(|n| n.to_string())
        .unwrap_or_default();
    let new = row
        .new_line_number
        .map(|n| n.to_string())
        .unwrap_or_default();
    let numbers = if continuation {
        " ".repeat(gutter - 3)
    } else if width >= 60 {
        let digits = (gutter - 5) / 2;
        format!("{old:>digits$} {new:>digits$} ")
    } else {
        let digits = gutter - 4;
        let number = if new.is_empty() { old } else { new };
        format!("{number:>digits$} ")
    };
    let (marker, color) = if continuation {
        ("↪", theme::MUTED)
    } else {
        match row.kind {
            RowKind::Added => ("+", theme::ADDED),
            RowKind::Removed => ("−", theme::REMOVED),
            _ => (" ", theme::MUTED),
        }
    };
    vec![
        Span::styled(numbers, theme::color(theme::MUTED)),
        Span::styled(marker, theme::strong(color)),
        Span::styled("│ ", theme::color(theme::BORDER)),
    ]
}

fn horizontal_range(text: &str, offset: usize, width: usize) -> (Range<usize>, usize) {
    let (mut column, mut used, mut padding) = (0, 0, 0);
    let (mut start, mut end) = (text.len(), text.len());
    for (byte, grapheme) in text.grapheme_indices(true) {
        let size = grapheme.width();
        if column < offset {
            column += size;
            if column > offset {
                padding = (column - offset).min(width);
                used = padding;
            }
            continue;
        }
        if used + size > width {
            break;
        }
        start = start.min(byte);
        end = byte + grapheme.len();
        used += size;
    }
    (start..end, padding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_scroll_preserves_wide_graphemes() {
        let text = "界hello";
        let (range, padding) = horizontal_range(text, 1, 4);
        assert_eq!(padding, 1);
        assert_eq!(&text[range], "hel");
    }
}
