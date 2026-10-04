use gtl_wire::terminal_diff::File;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, HighlightSpacing, List, ListItem, Paragraph, Scrollbar, ScrollbarState,
    },
};
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use super::{Mode, Pager, document::printable, screen::Target, theme};

pub(super) struct FileList {
    width: u16,
    details: bool,
    widget: List<'static>,
}

pub(super) fn draw(frame: &mut Frame<'_>, pager: &mut Pager, area: Rect) {
    let focused = matches!(pager.mode, Mode::Files);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::color(if focused {
            theme::ACCENT
        } else {
            theme::BORDER
        }))
        .title(Line::styled(
            format!(
                " FILES {}/{} ",
                pager.browser.matches.len(),
                pager.document.files.len()
            ),
            theme::strong(theme::ACCENT),
        ))
        .title_bottom(Line::styled(
            " Enter opens · / filter · v reviewed ",
            theme::color(theme::MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let filter = Rect::new(inner.x, inner.y, inner.width, 1);
    let label = if pager.browser.filter.is_empty() && !pager.browser.editing {
        " / Filter files… ".to_owned()
    } else {
        format!(
            " /{}{}",
            printable(&pager.browser.filter),
            if pager.browser.editing { "▏" } else { "" }
        )
    };
    frame.render_widget(
        Paragraph::new(label).style(theme::surface(theme::SURFACE)),
        filter,
    );
    pager.screen.targets.push((filter, Target::Filter));
    let list_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width.saturating_sub(1),
        inner.height.saturating_sub(1),
    );
    list(frame, pager, list_area, focused);
}

fn list(frame: &mut Frame<'_>, pager: &mut Pager, list_area: Rect, focused: bool) {
    let matches = &pager.browser.matches;
    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching files").style(theme::color(theme::MUTED)),
            list_area,
        );
        return;
    }
    if !focused {
        let current = pager.layout.entry_at(pager.offset).map(|entry| entry.file);
        pager
            .browser
            .state
            .select(current.and_then(|file| matches.binary_search(&file).ok()));
    }
    let item_height = if list_area.height >= 2 { 2 } else { 1 };
    let width = list_area.width.saturating_sub(2);
    let details = item_height == 2;
    if pager
        .browser
        .list
        .as_ref()
        .is_some_and(|list| list.width != width || list.details != details)
    {
        pager.browser.list = None;
    }
    let list = pager.browser.list.get_or_insert_with(|| FileList {
        width,
        details,
        widget: List::new(
            matches
                .iter()
                .map(|file| item(&pager.document.files[*file], width, details)),
        )
        .highlight_symbol("▌ ")
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(theme::selection())
        .scroll_padding(1),
    });
    frame.render_stateful_widget(&list.widget, list_area, &mut pager.browser.state);
    for (index, file) in matches
        .iter()
        .enumerate()
        .skip(pager.browser.state.offset())
        .take(usize::from(list_area.height) / item_height)
    {
        let y = list_area.y
            + u16::try_from((index - pager.browser.state.offset()) * item_height)
                .unwrap_or(u16::MAX);
        if y >= list_area.bottom() {
            break;
        }
        pager.screen.targets.push((
            Rect::new(
                list_area.x,
                y,
                list_area.width,
                (list_area.bottom() - y).min(u16::try_from(item_height).unwrap_or(1)),
            ),
            Target::File(*file),
        ));
    }
    if matches.len() <= usize::from(list_area.height) / item_height {
        return;
    }
    let mut scroll = ScrollbarState::new(matches.len())
        .position(pager.browser.state.offset())
        .viewport_content_length(usize::from(list_area.height) / item_height);
    let scroll_area = Rect::new(list_area.right(), list_area.y, 1, list_area.height);
    frame.render_stateful_widget(
        Scrollbar::default()
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(theme::color(theme::ACCENT))
            .track_style(theme::color(theme::BORDER)),
        scroll_area,
        &mut scroll,
    );
    pager
        .screen
        .targets
        .push((scroll_area, Target::BrowserScrollbar));
}

fn item(file: &File, width: u16, details: bool) -> ListItem<'static> {
    let path = printable(&file.path);
    let (directory, name) = path.rsplit_once('/').unwrap_or(("repository root", &path));
    let changes = format!("+{} −{}", file.added, file.removed);
    let available = usize::from(width).saturating_sub(changes.width() + 1);
    let name = if file.review.as_ref().is_some_and(|review| review.reviewed) {
        format!("✓ {name}")
    } else {
        name.to_owned()
    };
    let name = fit(&name, available, false);
    let spacing = usize::from(width).saturating_sub(name.width() + changes.width());
    let mut lines = vec![Line::from(vec![
        Span::styled(name, theme::strong(theme::TEXT)),
        Span::raw(" ".repeat(spacing)),
        Span::styled(format!("+{}", file.added), theme::color(theme::ADDED)),
        Span::styled(format!(" −{}", file.removed), theme::color(theme::REMOVED)),
    ])];
    if details {
        lines.push(Line::styled(
            fit(directory, usize::from(width), true),
            theme::color(theme::MUTED),
        ));
    }
    ListItem::new(Text::from(lines))
}

pub(super) fn fit(text: &str, width: usize, tail: bool) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = Vec::new();
    let mut used = 1;
    let graphemes = text.graphemes(true).collect::<Vec<_>>();
    let order: Box<dyn Iterator<Item = &&str>> = if tail {
        Box::new(graphemes.iter().rev())
    } else {
        Box::new(graphemes.iter())
    };
    for grapheme in order {
        if used + grapheme.width() > width {
            break;
        }
        result.push(*grapheme);
        used += grapheme.width();
    }
    if tail {
        result.reverse();
        format!("…{}", result.concat())
    } else {
        format!("{}…", result.concat())
    }
}
