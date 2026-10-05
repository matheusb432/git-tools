use gtl_wire::terminal_diff::{File, Row, RowKind};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};

use super::{Document, Pager, input, render};

#[test]
fn review_key_marks_the_visible_file_or_the_selected_browser_file() {
    let mut pager = Pager::default();
    pager.replace(document());
    pager.open_file(1);
    let key = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
    assert!(matches!(
        input::handle(&mut pager, key),
        input::Action::ToggleReview(1)
    ));
    pager.focus_files();
    pager.browser.state.select(Some(0));
    assert!(matches!(
        input::handle(&mut pager, key),
        input::Action::ToggleReview(0)
    ));
    pager.browser.editing = true;
    assert!(matches!(
        input::handle(&mut pager, key),
        input::Action::Continue
    ));
    assert_eq!(pager.browser.filter, "v");
}

fn row(number: u32, text: &str) -> Row {
    Row {
        kind: RowKind::Context,
        text: text.into(),
        old_line_number: Some(number),
        new_line_number: Some(number),
        syntax: Vec::new(),
    }
}

fn document() -> Document {
    let full = (1..=80)
        .map(|number| {
            row(
                number,
                &if number == 30 {
                    "needle 界 e\u{301} long line with repeated words to wrap on the phone".into()
                } else {
                    format!("context {number}")
                },
            )
        })
        .collect::<Vec<_>>();
    let mut compact = vec![Row {
        kind: RowKind::Hunk,
        text: "@@ -28,5 +28,5 @@".into(),
        old_line_number: None,
        new_line_number: None,
        syntax: Vec::new(),
    }];
    compact.extend_from_slice(&full[27..33]);
    Document {
        title: "Repository · HEAD".into(),
        notes: Vec::new(),
        files: vec![
            File {
                review: None,
                path: "src/first.rs".into(),
                added: 1,
                removed: 1,
                compact,
                full,
            },
            File {
                review: None,
                path: "src/second.rs".into(),
                added: 1,
                removed: 0,
                compact: vec![row(1, "another needle")],
                full: Vec::new(),
            },
        ],
    }
}

fn press(pager: &mut Pager, key: char) {
    let _ = input::handle(pager, KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
}

fn screen_buffer(pager: &mut Pager, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    pager.resize(width, height);
    terminal
        .draw(|frame| render::draw(frame, pager, false))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn screen(pager: &mut Pager, width: u16, height: u16) -> String {
    screen_buffer(pager, width, height)
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>()
}

#[test]
fn wrapped_source_keeps_every_character_at_the_minimum_terminal_width() {
    let source = "abcdefghijklmnopqrstuvwx";
    let mut document = document();
    document.files.truncate(1);
    document.files[0].path = "x.rs".into();
    document.files[0].compact = vec![row(1, source)];
    let mut pager = Pager::default();
    pager.resize(24, 12);
    pager.replace(document);

    for width in [24, 25] {
        let buffer = screen_buffer(&mut pager, width, 12);
        let content = pager.screen.content;
        let gutter = u16::try_from(pager.layout.gutter).unwrap();
        let mut rendered = String::new();
        for row in content.y..content.bottom() {
            for column in content.x + gutter..content.right() {
                rendered.push_str(buffer[(column, row)].symbol());
            }
        }
        assert!(rendered.contains(source), "{rendered}");
    }
}

#[test]
fn phone_layout_preserves_source_across_context_wrap_resize_and_refresh() {
    let mut pager = Pager::default();
    pager.resize(40, 12);
    pager.replace(document());
    pager.set_search("needle".into());
    pager.find(true, true);
    assert!(screen(&mut pager, 40, 12).contains("needle"));
    press(&mut pager, 'c');
    assert!(screen(&mut pager, 40, 12).contains("context 31"));
    press(&mut pager, 'w');
    assert!(screen(&mut pager, 100, 20).contains("needle"));
    assert!(screen(&mut pager, 32, 10).contains("needle"));
    let mut refreshed = document();
    refreshed.files[0].full[29].text = "refreshed needle".into();
    refreshed.files.insert(
        0,
        File {
            review: None,
            path: "before.txt".into(),
            added: 0,
            removed: 0,
            compact: Vec::new(),
            full: Vec::new(),
        },
    );
    pager.replace(refreshed);
    assert!(screen(&mut pager, 40, 12).contains("refreshed needle"));
}

#[test]
fn expanding_a_hunk_keeps_its_surrounding_source_in_view() {
    let mut pager = Pager::default();
    pager.resize(40, 12);
    pager.replace(document());
    press(&mut pager, ']');
    press(&mut pager, 'c');
    let view = screen(&mut pager, 40, 12);
    assert!(view.contains("context 28"));
    assert!(view.contains("needle"));
}

#[test]
fn repeated_search_advances_even_when_matches_share_the_last_screen() {
    let mut pager = Pager::default();
    pager.resize(100, 30);
    pager.replace(document());
    pager.set_search("needle".into());
    pager.find(true, true);
    let first = pager.offset;
    press(&mut pager, 'n');
    assert_ne!(pager.offset, first);
    assert!(pager.status.starts_with("Match 2/2"));
    press(&mut pager, 'n');
    assert_eq!(pager.offset, first);
    press(&mut pager, 'N');
    assert!(pager.status.starts_with("Match 2/2"));
    press(&mut pager, 'N');
    assert_eq!(pager.offset, first);
}

#[test]
fn search_results_follow_query_context_wrap_resize_and_refresh() {
    let mut pager = Pager::default();
    pager.resize(100, 30);
    let mut original = document();
    original.files[0].full[0].text = "wrapping prefix ".repeat(8);
    pager.replace(original);
    pager.set_search("CONTEXT 10".into());
    pager.find(true, true);
    assert!(pager.status.starts_with("No matches"));
    press(&mut pager, 'c');
    pager.find(true, true);
    assert!(pager.status.starts_with("Match 1/1"));
    assert!(screen(&mut pager, 100, 30).contains("context 10"));

    pager.set_search("long line".into());
    for (width, height) in [(32, 12), (120, 30)] {
        pager.resize(width, height);
        press(&mut pager, 'n');
        assert!(pager.status.starts_with("Match 1/1"));
        assert_eq!(
            pager
                .layout
                .source(
                    &pager.document,
                    pager.layout.entry_at(pager.offset).unwrap()
                )
                .unwrap()
                .new_line_number,
            Some(30)
        );
        press(&mut pager, 'w');
        press(&mut pager, 'n');
        assert!(pager.status.starts_with("Match 1/1"));
        assert_eq!(
            pager
                .layout
                .source(
                    &pager.document,
                    pager.layout.entry_at(pager.offset).unwrap()
                )
                .unwrap()
                .new_line_number,
            Some(30)
        );
        assert!(screen(&mut pager, width, height).contains("needle"));
    }

    let mut refreshed = document();
    refreshed.files[0].full[29].text = "replacement match".into();
    pager.replace(refreshed);
    press(&mut pager, 'n');
    assert!(pager.status.starts_with("No matches"));
    pager.set_search("replacement".into());
    press(&mut pager, 'n');
    assert!(pager.status.starts_with("Match 1/1"));
    assert!(screen(&mut pager, 120, 30).contains("replacement match"));
    pager.set_search(String::new());
    assert_eq!(pager.search.match_count(), 0);
}

#[test]
fn file_picker_filters_and_help_remains_accessible_on_short_screens() {
    let mut pager = Pager::default();
    pager.resize(40, 8);
    pager.replace(document());
    for key in "f/second".chars() {
        press(&mut pager, key);
    }
    let list = screen(&mut pager, 40, 8);
    assert!(list.contains("second.rs"));
    assert!(list.contains("FILES 1/2"));
    let _ = input::handle(
        &mut pager,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(screen(&mut pager, 40, 8).contains("second.rs"));
    press(&mut pager, '?');
    for _ in 0..80 {
        press(&mut pager, 'j');
    }
    assert!(screen(&mut pager, 40, 8).contains("Press ? or Esc"));
}

#[test]
fn scrolling_is_not_limited_to_u16_rows() {
    let mut document = document();
    document.files[0].compact = (1..=70_000)
        .map(|number| row(number, &format!("line {number}")))
        .collect();
    let mut pager = Pager::default();
    pager.resize(40, 12);
    pager.replace(document);
    press(&mut pager, 'G');
    assert!(screen(&mut pager, 40, 12).contains("line 70000"));
}

#[test]
fn file_list_updates_after_filter_resize_and_refresh() {
    let mut pager = Pager::default();
    pager.replace(document());
    pager.focus_files();
    assert!(screen(&mut pager, 120, 30).contains("FILES 2/2"));
    for key in "/first".chars() {
        press(&mut pager, key);
    }
    for (width, height) in [(120, 30), (40, 12), (120, 30)] {
        assert!(screen(&mut pager, width, height).contains("FILES 1/2"));
    }
    let mut refreshed = document();
    refreshed.files[0].path = "src/renamed.rs".into();
    refreshed.files[0].added = 42;
    pager.replace(refreshed);
    assert!(screen(&mut pager, 120, 30).contains("No matching files"));
    for _ in 0..5 {
        let _ = input::handle(
            &mut pager,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
    }
    let view = screen(&mut pager, 120, 30);
    assert!(view.contains("FILES 2/2"));
    assert!(view.contains("renamed.rs"));
    assert!(view.contains("+42"));
    assert!(!view.contains("first.rs"));
}

#[test]
fn mouse_targets_follow_toolbar_and_file_browser_after_resize() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    use super::screen::{Control, Target};

    let mut pager = Pager::default();
    pager.replace(document());
    for (width, height) in [(120, 30), (40, 12), (24, 12)] {
        screen(&mut pager, width, height);
        let targets = pager.screen.targets.clone();
        for (area, target) in &targets {
            assert!(area.right() <= width && area.bottom() <= height);
            assert!(
                targets
                    .iter()
                    .all(|(other, _)| area == other || area.intersection(*other).is_empty())
            );
            if *target == Target::Control(Control::Context) {
                let before = pager.full;
                let _ = input::mouse(
                    &mut pager,
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: area.x,
                        row: area.y,
                        modifiers: KeyModifiers::NONE,
                    },
                );
                assert_ne!(pager.full, before);
            }
        }
        pager.focus_files();
        screen(&mut pager, width, height);
        let file = pager
            .screen
            .targets
            .iter()
            .find(|(_, target)| *target == Target::File(0))
            .unwrap()
            .0;
        let _ = input::mouse(
            &mut pager,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: file.x,
                row: file.y,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(matches!(pager.mode, super::Mode::Pager));
    }
    if !pager.full {
        press(&mut pager, 'c');
    }
    screen(&mut pager, 120, 30);
    let content = pager.screen.content;
    let _ = input::mouse(
        &mut pager,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: content.x,
            row: content.y,
            modifiers: KeyModifiers::NONE,
        },
    );
    assert_eq!(pager.offset, 3);
}

#[test]
fn dragging_scrollbars_reaches_the_last_file_in_both_panes() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    use super::screen::Target;

    let mut document = document();
    for index in 0..30 {
        let mut file = document.files[1].clone();
        file.path = format!("src/extra{index}.rs");
        document.files.push(file);
    }
    let mut pager = Pager::default();
    pager.replace(document);
    for target in [Target::Scrollbar, Target::BrowserScrollbar] {
        if target == Target::BrowserScrollbar {
            pager.mode = super::Mode::Files;
            pager.browser.state.select(Some(0));
        }
        screen(&mut pager, 120, 14);
        let area = pager
            .screen
            .targets
            .iter()
            .find(|(_, candidate)| *candidate == target)
            .unwrap()
            .0;
        let _ = input::mouse(
            &mut pager,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: area.x,
                row: area.bottom() - 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(screen(&mut pager, 120, 14).contains("extra29.rs"));
        if target == Target::BrowserScrollbar {
            assert_eq!(pager.browser.state.selected(), Some(31));
        }
    }
}
