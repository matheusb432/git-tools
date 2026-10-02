use ratatui::layout::{Constraint, Layout, Rect};
use unicode_width::UnicodeWidthStr as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Files,
    Find,
    Wrap,
    Context,
    Previous,
    Next,
    Refresh,
    Help,
    Quit,
}

impl Control {
    pub fn label(self, wide: bool) -> &'static str {
        match (self, wide) {
            (Self::Files, true) => " f Files ",
            (Self::Files, false) => " Files ",
            (Self::Find, true) => " / Find ",
            (Self::Find, false) => " Find ",
            (Self::Wrap, true) => " w Wrap ",
            (Self::Wrap, false) => " Wrap ",
            (Self::Context, true) => " c Context ",
            (Self::Context, false) => " Full ",
            (Self::Previous, true) => " [ Prev ",
            (Self::Previous, false) => " ‹ ",
            (Self::Next, true) => " ] Next ",
            (Self::Next, false) => " › ",
            (Self::Refresh, true) => " r Refresh ",
            (Self::Refresh, false) => " Refresh ",
            (Self::Help, true) => " ? Help ",
            (Self::Help, false) => " Help ",
            (Self::Quit, true) => " q Quit ",
            (Self::Quit, false) => " Quit ",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Control(Control),
    File(usize),
    Filter,
    Scrollbar,
    BrowserScrollbar,
}

#[derive(Default)]
pub(super) struct Screen {
    pub area: Rect,
    pub header: Rect,
    pub body: Rect,
    pub pane: Rect,
    pub content: Rect,
    pub browser: Option<Rect>,
    pub status: Rect,
    pub buttons: Vec<(Rect, Control)>,
    pub targets: Vec<(Rect, Target)>,
}

impl Screen {
    pub fn new(area: Rect) -> Self {
        let wide = area.width >= 90;
        let controls = [
            Control::Files,
            Control::Find,
            Control::Wrap,
            Control::Context,
            Control::Previous,
            Control::Next,
            Control::Refresh,
            Control::Help,
            Control::Quit,
        ];
        let mut buttons = Vec::new();
        let (mut x, mut row) = (0, 0);
        for control in controls {
            let width = u16::try_from(control.label(wide).width())
                .unwrap_or(1)
                .min(area.width);
            if x + width > area.width {
                x = 0;
                row += 1;
            }
            buttons.push((Rect::new(area.x + x, row, width, 1), control));
            x += width + 1;
        }
        let [header, body, status, toolbar] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(row + 1),
        ])
        .areas(area);
        for (rect, _) in &mut buttons {
            rect.y = rect.y.saturating_add(toolbar.y);
        }
        let (browser, pane) = if area.width >= 100 && area.height >= 12 {
            let [browser, pane] = Layout::horizontal([
                Constraint::Length((area.width / 4).clamp(28, 36)),
                Constraint::Fill(1),
            ])
            .spacing(1)
            .areas(body);
            (Some(browser), pane)
        } else {
            (None, body)
        };
        let content = pane.inner(ratatui::layout::Margin {
            horizontal: 1,
            vertical: 1,
        });
        Self {
            area,
            header,
            body,
            pane,
            content,
            browser,
            status,
            buttons,
            targets: Vec::new(),
        }
    }

    pub fn target_at(&self, column: u16, row: u16) -> Option<Target> {
        self.targets
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains((column, row).into()))
            .map(|(_, target)| *target)
    }
}
