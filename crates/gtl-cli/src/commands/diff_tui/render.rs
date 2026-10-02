use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Scrollbar, ScrollbarState, Wrap},
};
use unicode_width::UnicodeWidthStr as _;

use super::{
    Mode, Pager, browser, code,
    document::printable,
    screen::{Control, Target},
    theme,
};

const HELP: &str = "READING A DIFF\n\nj/k or arrows   Scroll\nSpace/b        Page down/up\ng/G            Start/end\n[/]            Previous/next hunk\n{/}            Previous/next file\nf or Tab       Focus file browser\n/              Search\nn/N            Next/previous match\nw              Toggle wrapping\nh/l or arrows  Pan unwrapped lines\nc              Toggle full context\nr              Refresh comparison\nq or Ctrl-C    Quit\n\nMOUSE\n\nClick a control to activate it.\nClick a file to open it.\nClick Filter files to type a filter.\nScroll the wheel over either pane.\nClick or drag the diff scrollbar.\n\nEsc closes the current panel.\nPress ? or Esc to return";

pub(super) fn help_scroll_max(width: u16, page: usize) -> u16 {
    let rows: usize = HELP
        .lines()
        .map(|line| line.width().div_ceil(usize::from(width).max(1)).max(1))
        .sum();
    u16::try_from(rows.saturating_sub(page)).unwrap_or(u16::MAX)
}

pub(super) fn draw(frame: &mut Frame<'_>, pager: &mut Pager, loading: bool) {
    let area = frame.area();
    pager.screen.targets.clear();
    frame.render_widget(Block::new().style(theme::surface(theme::BACKGROUND)), area);
    if area.width < 24 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Widen terminal (24×8) · q quit").wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    header(frame, pager, loading);
    match pager.mode {
        Mode::Help { offset } => help(frame, pager, offset),
        Mode::Files if pager.screen.browser.is_none() => {
            browser::draw(frame, pager, pager.screen.body);
        }
        _ => {
            if let Some(area) = pager.screen.browser {
                browser::draw(frame, pager, area);
            }
            diff(frame, pager, loading);
        }
    }
    status(frame, pager, loading);
    controls(frame, pager);
}

fn header(frame: &mut Frame<'_>, pager: &Pager, loading: bool) {
    let title = if pager.document.title.is_empty() {
        "Terminal review"
    } else {
        &pager.document.title
    };
    let label = if loading { "  ◌ loading" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " gtl ",
                theme::surface(theme::ACCENT)
                    .fg(if theme::enabled() {
                        theme::BACKGROUND
                    } else {
                        ratatui::style::Color::Reset
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" DIFF ", theme::strong(theme::TEXT)),
            Span::styled(
                format!(
                    " {}{label}",
                    browser::fit(
                        title,
                        usize::from(pager.screen.header.width).saturating_sub(13 + label.width()),
                        true
                    )
                ),
                theme::color(theme::MUTED),
            ),
        ]))
        .style(theme::surface(theme::SURFACE)),
        pager.screen.header,
    );
}

fn diff(frame: &mut Frame<'_>, pager: &mut Pager, loading: bool) {
    let path = pager.layout.entry_at(pager.offset).map_or_else(
        || "Changes".into(),
        |entry| printable(&pager.document.files[entry.file].path),
    );
    let title = format!(
        " {} ",
        browser::fit(
            &path,
            usize::from(pager.screen.pane.width).saturating_sub(6),
            true
        )
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::color(theme::BORDER))
        .title(Line::styled(title, theme::strong(theme::TEXT)));
    frame.render_widget(block, pager.screen.pane);
    if pager.document.files.is_empty() {
        let message = if loading {
            "\nReading your comparison…"
        } else {
            "\n✓  No file changes\n\nThis comparison is clean."
        };
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(theme::color(theme::MUTED)),
            pager.screen.content,
        );
    } else {
        code::draw(frame, pager, pager.screen.content);
    }
    if pager.layout.screen_rows <= pager.page {
        return;
    }
    let scroll_area = Rect::new(
        pager.screen.pane.right().saturating_sub(1),
        pager.screen.content.y,
        1,
        pager.screen.content.height,
    );
    let mut state = ScrollbarState::new(pager.layout.screen_rows)
        .position(pager.offset)
        .viewport_content_length(pager.page);
    frame.render_stateful_widget(
        Scrollbar::default()
            .begin_symbol(Some("▴"))
            .end_symbol(Some("▾"))
            .thumb_style(theme::color(theme::ACCENT))
            .track_style(theme::color(theme::BORDER)),
        scroll_area,
        &mut state,
    );
    pager.screen.targets.push((scroll_area, Target::Scrollbar));
}

fn help(frame: &mut Frame<'_>, pager: &Pager, offset: u16) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::color(theme::ACCENT))
        .title(Line::styled(" HELP ", theme::strong(theme::ACCENT)));
    frame.render_widget(
        Paragraph::new(HELP)
            .style(theme::color(theme::TEXT))
            .wrap(Wrap { trim: false })
            .scroll((offset, 0))
            .block(block),
        pager.screen.body,
    );
}

fn status(frame: &mut Frame<'_>, pager: &Pager, loading: bool) {
    let text = match &pager.mode {
        Mode::Search(query) => Line::from(vec![
            Span::styled(" / ", theme::strong(theme::ACCENT)),
            Span::raw(format!("{}▏", printable(query))),
        ]),
        _ if loading => Line::styled(" Refreshing comparison…", theme::color(theme::ACCENT)),
        _ if !pager.status.is_empty() => Line::styled(
            format!(" {}", printable(&pager.status)),
            theme::color(theme::MUTED),
        ),
        _ => summary(pager),
    };
    frame.render_widget(Paragraph::new(text), pager.screen.status);
}

fn summary(pager: &Pager) -> Line<'static> {
    let (added, removed) = (pager.added, pager.removed);
    let progress = pager
        .offset
        .saturating_add(pager.page)
        .min(pager.layout.screen_rows)
        * 100
        / pager.layout.screen_rows.max(1);
    Line::from(vec![
        Span::styled(
            format!(" {} files ", pager.document.files.len()),
            theme::color(theme::MUTED),
        ),
        Span::styled(format!("+{added} "), theme::color(theme::ADDED)),
        Span::styled(format!("−{removed}"), theme::color(theme::REMOVED)),
        Span::styled(
            format!(
                " · {} · {progress}%",
                if pager.full { "full" } else { "hunks" }
            ),
            theme::color(theme::MUTED),
        ),
    ])
}

fn controls(frame: &mut Frame<'_>, pager: &mut Pager) {
    for &(area, control) in &pager.screen.buttons {
        let selected = match control {
            Control::Wrap => pager.wrap,
            Control::Context => pager.full,
            Control::Files => matches!(pager.mode, Mode::Files),
            Control::Find => matches!(pager.mode, Mode::Search(_)),
            Control::Help => matches!(pager.mode, Mode::Help { .. }),
            _ => false,
        };
        let style = if selected {
            theme::surface(theme::SELECTED).patch(theme::strong(theme::ACCENT))
        } else {
            theme::surface(theme::SURFACE)
        };
        let style = if pager.hover == Some(Target::Control(control)) {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        };
        frame.render_widget(
            Paragraph::new(control.label(pager.screen.area.width >= 90)).style(style),
            area,
        );
        pager.screen.targets.push((area, Target::Control(control)));
    }
}
