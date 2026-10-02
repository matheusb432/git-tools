use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use super::{
    Mode, Pager,
    screen::{Control, Target},
};

pub(super) enum Action {
    Continue,
    Refresh,
    Quit,
}

pub(super) fn handle(pager: &mut Pager, key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }
    match std::mem::take(&mut pager.mode) {
        Mode::Pager => pager_key(pager, key),
        Mode::Help { offset } => {
            help_key(pager, key, offset);
            Action::Continue
        }
        Mode::Search(query) => {
            search_key(pager, key, query);
            Action::Continue
        }
        Mode::Files => {
            picker_key(pager, key);
            Action::Continue
        }
    }
}

fn help_key(pager: &mut Pager, key: KeyEvent, mut offset: u16) {
    let page = u16::try_from(pager.page).unwrap_or(u16::MAX);
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | '?') | KeyCode::Enter => return,
        KeyCode::Down | KeyCode::Char('j') => offset = offset.saturating_add(1),
        KeyCode::Up | KeyCode::Char('k') => offset = offset.saturating_sub(1),
        KeyCode::PageDown | KeyCode::Char(' ') => offset = offset.saturating_add(page),
        KeyCode::PageUp | KeyCode::Char('b') => offset = offset.saturating_sub(page),
        _ => {}
    }
    pager.mode = Mode::Help {
        offset: offset.min(super::render::help_scroll_max(
            pager.screen.body.width.saturating_sub(2),
            pager.page,
        )),
    };
}

fn search_key(pager: &mut Pager, key: KeyEvent, mut query: String) {
    match key.code {
        KeyCode::Esc => return,
        KeyCode::Enter => {
            pager.set_search(query);
            pager.find(true, true);
            return;
        }
        KeyCode::Backspace => {
            query.pop();
        }
        KeyCode::Char(character) if text_key(key) && query.len() < 1024 => query.push(character),
        _ => {}
    }
    pager.mode = Mode::Search(query);
}

fn text_key(key: KeyEvent) -> bool {
    !key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

fn pager_key(pager: &mut Pager, key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('d') => pager.scroll(true, (pager.page / 2).max(1)),
            KeyCode::Char('u') => pager.scroll(false, (pager.page / 2).max(1)),
            _ => {}
        }
        return Action::Continue;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
        KeyCode::Char('r') => return Action::Refresh,
        KeyCode::Down | KeyCode::Char('j') => pager.scroll(true, 1),
        KeyCode::Up | KeyCode::Char('k') => pager.scroll(false, 1),
        KeyCode::PageDown | KeyCode::Char(' ') => pager.scroll(true, pager.page),
        KeyCode::PageUp | KeyCode::Char('b') => pager.scroll(false, pager.page),
        KeyCode::Home | KeyCode::Char('g') => {
            pager.search.reset_cursor();
            pager.offset = 0;
        }
        KeyCode::End | KeyCode::Char('G') => {
            pager.search.reset_cursor();
            pager.offset = pager.layout.screen_rows.saturating_sub(pager.page);
            pager.clamp();
        }
        KeyCode::Left | KeyCode::Char('h') if !pager.wrap => pan(pager, false),
        KeyCode::Right | KeyCode::Char('l') if !pager.wrap => pan(pager, true),
        KeyCode::Char('w') => return control(pager, Control::Wrap),
        KeyCode::Char('c') => return control(pager, Control::Context),
        KeyCode::Char(']') => pager.jump(false, true),
        KeyCode::Char('[') => pager.jump(false, false),
        KeyCode::Char('}') => pager.jump(true, true),
        KeyCode::Char('{') => pager.jump(true, false),
        KeyCode::Char('f') | KeyCode::Tab => pager.focus_files(),
        KeyCode::Char('/') => pager.mode = Mode::Search(String::new()),
        KeyCode::Char('n') => pager.find(true, false),
        KeyCode::Char('N') => pager.find(false, false),
        KeyCode::Char('?') => pager.mode = Mode::Help { offset: 0 },
        _ => {}
    }
    Action::Continue
}

fn picker_key(pager: &mut Pager, key: KeyEvent) {
    pager.mode = Mode::Files;
    match key.code {
        KeyCode::Esc | KeyCode::Tab => {
            pager.mode = Mode::Pager;
            pager.browser.editing = false;
        }
        KeyCode::Enter => {
            if let Some(file) = pager
                .browser
                .state
                .selected()
                .and_then(|index| pager.browser.matches.get(index))
            {
                pager.open_file(*file);
            }
        }
        KeyCode::Down => select_file(pager, true, 1),
        KeyCode::Up => select_file(pager, false, 1),
        KeyCode::Char('j') if !pager.browser.editing => select_file(pager, true, 1),
        KeyCode::Char('k') if !pager.browser.editing => select_file(pager, false, 1),
        KeyCode::Char('q') if !pager.browser.editing => pager.mode = Mode::Pager,
        KeyCode::Char('/') if !pager.browser.editing => pager.browser.editing = true,
        KeyCode::Backspace if pager.browser.editing => {
            pager.browser.filter.pop();
            pager.filter_files();
            pager.browser.state.select(Some(0));
        }
        KeyCode::Char(character)
            if pager.browser.editing && text_key(key) && pager.browser.filter.len() < 1024 =>
        {
            pager.browser.filter.push(character);
            pager.filter_files();
            pager.browser.state.select(Some(0));
        }
        _ => {}
    }
}

fn select_file(pager: &mut Pager, forward: bool, amount: usize) {
    let count = pager.browser.matches.len();
    let current = pager.browser.state.selected().unwrap_or(0);
    let selected = if forward {
        current.saturating_add(amount)
    } else {
        current.saturating_sub(amount)
    };
    pager
        .browser
        .state
        .select((count > 0).then_some(selected.min(count.saturating_sub(1))));
}

fn pan(pager: &mut Pager, forward: bool) {
    pager.horizontal = if forward {
        pager
            .horizontal
            .saturating_add(4)
            .min(gtl_wire::terminal_diff::ROW_BYTES_MAX * 4)
    } else {
        pager.horizontal.saturating_sub(4)
    };
}

fn control(pager: &mut Pager, control: Control) -> Action {
    match control {
        Control::Files if matches!(pager.mode, Mode::Files) => pager.mode = Mode::Pager,
        Control::Files => pager.focus_files(),
        Control::Find => pager.mode = Mode::Search(String::new()),
        Control::Wrap => {
            pager.wrap = !pager.wrap;
            pager.horizontal = 0;
            pager.rebuild();
        }
        Control::Context => {
            pager.full = !pager.full;
            pager.rebuild();
        }
        Control::Previous => pager.jump(false, false),
        Control::Next => pager.jump(false, true),
        Control::Refresh => return Action::Refresh,
        Control::Help if matches!(pager.mode, Mode::Help { .. }) => pager.mode = Mode::Pager,
        Control::Help => pager.mode = Mode::Help { offset: 0 },
        Control::Quit => return Action::Quit,
    }
    Action::Continue
}

pub(super) fn mouse(pager: &mut Pager, mouse: MouseEvent) -> Action {
    let target = pager.screen.target_at(mouse.column, mouse.row);
    pager.hover = target.filter(|target| matches!(target, Target::Control(_)));
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => match target {
            Some(Target::Control(button)) => return control(pager, button),
            Some(Target::File(file)) => pager.open_file(file),
            Some(Target::Filter) => {
                pager.focus_files();
                pager.browser.editing = true;
            }
            Some(Target::Scrollbar) => scrollbar(pager, mouse.row),
            Some(Target::BrowserScrollbar) => browser_scrollbar(pager, mouse.row),
            None => {}
        },
        MouseEventKind::Drag(MouseButton::Left) if target == Some(Target::Scrollbar) => {
            scrollbar(pager, mouse.row);
        }
        MouseEventKind::Drag(MouseButton::Left) if target == Some(Target::BrowserScrollbar) => {
            browser_scrollbar(pager, mouse.row);
        }
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => wheel(pager, mouse),
        MouseEventKind::ScrollLeft if !pager.wrap => pan(pager, false),
        MouseEventKind::ScrollRight if !pager.wrap => pan(pager, true),
        _ => {}
    }
    Action::Continue
}

fn browser_scrollbar(pager: &mut Pager, row: u16) {
    let Some((area, _)) = pager
        .screen
        .targets
        .iter()
        .find(|(_, target)| *target == Target::BrowserScrollbar)
    else {
        return;
    };
    let count = pager.browser.matches.len();
    let position = usize::from(row.saturating_sub(area.y));
    let height = usize::from(area.height.saturating_sub(1)).max(1);
    pager.mode = Mode::Files;
    pager
        .browser
        .state
        .select((count > 0).then_some(position * count.saturating_sub(1) / height));
}

fn wheel(pager: &mut Pager, mouse: MouseEvent) {
    let down = mouse.kind == MouseEventKind::ScrollDown;
    if let Mode::Help { offset } = pager.mode {
        help_key(
            pager,
            KeyEvent::new(
                if down { KeyCode::Down } else { KeyCode::Up },
                KeyModifiers::NONE,
            ),
            offset,
        );
    } else if pager
        .screen
        .browser
        .is_some_and(|area| area.contains((mouse.column, mouse.row).into()))
        || matches!(pager.mode, Mode::Files) && pager.screen.browser.is_none()
    {
        pager.mode = Mode::Files;
        select_file(pager, down, 3);
    } else if pager
        .screen
        .content
        .contains((mouse.column, mouse.row).into())
    {
        pager.scroll(down, 3);
    }
}

fn scrollbar(pager: &mut Pager, row: u16) {
    let position = usize::from(row.saturating_sub(pager.screen.content.y));
    let height = usize::from(pager.screen.content.height.saturating_sub(1)).max(1);
    pager.search.reset_cursor();
    pager.offset = position * pager.layout.screen_rows.saturating_sub(1) / height;
    pager.clamp();
}
