use std::io::IsTerminal as _;

use dialoguer::console::{Style, measure_text_width, strip_ansi_codes};

pub(crate) fn stdout_color() -> bool {
    terminal_color(std::io::stdout().is_terminal())
}

pub(crate) fn stderr_color() -> bool {
    terminal_color(std::io::stderr().is_terminal())
}

fn terminal_color(terminal: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        false
    } else {
        terminal
            || std::env::var("CLICOLOR_FORCE").is_ok_and(|value| !value.is_empty() && value != "0")
    }
}

pub(crate) fn heading(text: &str, color: bool) -> String {
    Style::new()
        .cyan()
        .bold()
        .force_styling(color)
        .apply_to(text)
        .to_string()
}

pub(crate) fn single_line(value: &str) -> String {
    strip_ansi_codes(value)
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

pub(crate) fn sentence(value: &str) -> String {
    let text = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let text = single_line(&text);
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        format!("{}{}", first.to_uppercase(), characters.as_str())
    })
}

pub(crate) fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

pub(crate) fn table<const COLUMNS: usize>(
    headers: [&str; COLUMNS],
    rows: &[[String; COLUMNS]],
    color: bool,
) -> String {
    let widths: [usize; COLUMNS] = std::array::from_fn(|column| {
        rows.iter()
            .map(|row| measure_text_width(&single_line(&row[column])))
            .chain([measure_text_width(headers[column])])
            .max()
            .unwrap_or_default()
    });
    let render = |cells: [&str; COLUMNS]| {
        cells
            .into_iter()
            .enumerate()
            .map(|(column, value)| {
                let value = single_line(value);
                if column + 1 == COLUMNS {
                    value
                } else {
                    let padding = widths[column].saturating_sub(measure_text_width(&value));
                    format!("{value}{}", " ".repeat(padding))
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    std::iter::once(heading(&render(headers), color))
        .chain(
            rows.iter()
                .map(|row| render(std::array::from_fn(|column| row[column].as_str()))),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_aligns_terminal_columns_and_sanitizes_cells() {
        let rows = [
            ["界".into(), "done\nnext".into()],
            ["app".into(), "done".into()],
        ];
        assert_eq!(
            table(["Project", "Result"], &rows, false),
            "Project  Result\n界       done next\napp      done"
        );
    }

    #[test]
    fn table_measures_cells_after_flattening_control_characters() {
        let rows = [
            ["two\nparts".into(), "ready".into()],
            ["one".into(), "ready".into()],
        ];
        assert_eq!(
            table(["Project", "Result"], &rows, false),
            "Project    Result\ntwo parts  ready\none        ready"
        );
    }
}
