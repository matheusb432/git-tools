use gtl_wire::{terminal_diff::RowKind, viewer::ViewerSyntaxClass};
use ratatui::style::{Color, Modifier, Style};

pub(super) const BACKGROUND: Color = Color::Rgb(12, 18, 30);
pub(super) const SURFACE: Color = Color::Rgb(20, 29, 45);
pub(super) const BORDER: Color = Color::Rgb(53, 69, 92);
pub(super) const TEXT: Color = Color::Rgb(219, 228, 240);
pub(super) const MUTED: Color = Color::Rgb(137, 156, 181);
pub(super) const ACCENT: Color = Color::Rgb(94, 224, 202);
pub(super) const ADDED: Color = Color::Rgb(135, 229, 172);
pub(super) const REMOVED: Color = Color::Rgb(255, 155, 170);
pub(super) const SELECTED: Color = Color::Rgb(35, 58, 77);

pub(super) fn color(foreground: Color) -> Style {
    if enabled() {
        Style::default().fg(foreground)
    } else {
        Style::default()
    }
}

pub(super) fn surface(background: Color) -> Style {
    if enabled() {
        color(TEXT).bg(background)
    } else {
        Style::default()
    }
}

pub(super) fn strong(foreground: Color) -> Style {
    color(foreground).add_modifier(Modifier::BOLD)
}

pub(super) fn selection() -> Style {
    let style = if enabled() {
        Style::default().bg(SELECTED)
    } else {
        Style::default()
    };
    style.add_modifier(Modifier::BOLD)
}

pub(super) fn enabled() -> bool {
    static ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
    });
    *ENABLED
}

pub(super) fn row(kind: RowKind) -> Style {
    match kind {
        RowKind::Added => surface(Color::Rgb(17, 43, 39)),
        RowKind::Removed => surface(Color::Rgb(48, 27, 39)),
        RowKind::Hunk => surface(SURFACE).patch(color(ACCENT)),
        RowKind::Meta => color(MUTED),
        RowKind::Context => color(TEXT),
    }
}

pub(super) fn syntax(class: ViewerSyntaxClass) -> Style {
    match class {
        ViewerSyntaxClass::Keyword => strong(Color::Rgb(199, 169, 255)),
        ViewerSyntaxClass::String => color(Color::Rgb(171, 220, 145)),
        ViewerSyntaxClass::Comment => color(MUTED).add_modifier(Modifier::ITALIC),
        ViewerSyntaxClass::Type => color(Color::Rgb(246, 205, 132)),
        ViewerSyntaxClass::Function => color(Color::Rgb(130, 199, 255)),
        ViewerSyntaxClass::Number | ViewerSyntaxClass::Constant => color(Color::Rgb(250, 174, 123)),
        ViewerSyntaxClass::Operator => color(Color::Rgb(154, 216, 220)),
        ViewerSyntaxClass::Tag => color(REMOVED),
        ViewerSyntaxClass::Variable => color(TEXT),
    }
}
