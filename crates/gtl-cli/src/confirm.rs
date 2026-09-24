use std::{fmt, io::IsTerminal as _};

use dialoguer::{
    Confirm,
    console::Term,
    theme::{ColorfulTheme, Theme},
};

use crate::{ExitCode, output};

pub(crate) struct Detail {
    label: &'static str,
    value: String,
}

impl Detail {
    pub(crate) fn new(label: &'static str, value: impl fmt::Display) -> Self {
        Self {
            label,
            value: output::single_line(&value.to_string()),
        }
    }
}

pub(crate) struct Dialog {
    heading: &'static str,
    details: Vec<Detail>,
    question: String,
    table: Option<String>,
}

impl Dialog {
    pub(crate) fn new(
        heading: &'static str,
        details: Vec<Detail>,
        question: impl Into<String>,
    ) -> Self {
        Self {
            heading,
            details,
            question: output::single_line(&question.into()),
            table: None,
        }
    }

    pub(crate) fn with_table(mut self, table: String) -> Self {
        self.table = Some(table);
        self
    }

    pub(crate) fn render(&self, heading: &str, color: bool) -> String {
        let width = self
            .details
            .iter()
            .map(|detail| detail.label.len())
            .max()
            .unwrap_or_default();
        let mut lines = vec![output::heading(heading, color), String::new()];
        lines.extend(self.details.iter().map(|detail| {
            format!(
                "  {}  {}",
                output::heading(&format!("{:<width$}", detail.label), color),
                detail.value
            )
        }));
        if let Some(table) = &self.table {
            if !self.details.is_empty() {
                lines.push(String::new());
            }
            lines.push(table.clone());
        }
        lines.join("\n")
    }

    fn interact(&self) -> dialoguer::Result<Option<bool>> {
        let terminal = Term::stderr();
        terminal.write_line(&self.render(self.heading, output::stderr_color()))?;
        terminal.write_line("")?;
        let theme = ConfirmationTheme {
            color: output::stderr_color(),
            theme: ColorfulTheme::default(),
        };
        let answer = Confirm::with_theme(&theme)
            .with_prompt(&self.question)
            .default(true)
            .interact_on_opt(&terminal);
        if answer.is_err() {
            terminal.show_cursor()?;
        }
        answer
    }
}

pub(crate) fn stdin_is_terminal() -> bool {
    std::io::stdin().is_terminal()
}

pub(crate) fn request(command: &str, bypass: bool, dialog: &Dialog) -> Result<(), ExitCode> {
    if bypass {
        return Ok(());
    }
    if !stdin_is_terminal() || !std::io::stderr().is_terminal() {
        eprintln!("{command}: interactive confirmation requires a terminal; pass --yes to proceed");
        return Err(ExitCode::Usage);
    }
    match dialog.interact() {
        Ok(Some(true)) => Ok(()),
        Ok(Some(false) | None) => Err(ExitCode::Ok),
        Err(dialoguer::Error::IO(error)) if error.kind() == std::io::ErrorKind::Interrupted => {
            eprintln!("{command}: cancelled");
            Err(ExitCode::Ok)
        }
        Err(error) => {
            eprintln!("{command}: confirmation failed: {error}");
            Err(ExitCode::Failed)
        }
    }
}

struct ConfirmationTheme {
    theme: ColorfulTheme,
    color: bool,
}

impl ConfirmationTheme {
    fn write(
        &self,
        formatter: &mut dyn fmt::Write,
        render: impl FnOnce(&mut String) -> fmt::Result,
    ) -> fmt::Result {
        let mut text = String::new();
        render(&mut text)?;
        if self.color {
            formatter.write_str(&text)
        } else {
            formatter.write_str(&dialoguer::console::strip_ansi_codes(&text))
        }
    }
}

impl Theme for ConfirmationTheme {
    fn format_confirm_prompt(
        &self,
        formatter: &mut dyn fmt::Write,
        prompt: &str,
        default: Option<bool>,
    ) -> fmt::Result {
        self.write(formatter, |text| {
            self.theme.format_confirm_prompt(text, prompt, default)
        })
    }

    fn format_confirm_prompt_selection(
        &self,
        formatter: &mut dyn fmt::Write,
        prompt: &str,
        selection: Option<bool>,
    ) -> fmt::Result {
        self.write(formatter, |text| {
            if selection == Some(true) {
                self.theme
                    .format_confirm_prompt_selection(text, prompt, selection)
            } else {
                use fmt::Write as _;
                write!(
                    text,
                    "{} {} {} {}",
                    self.theme.error_prefix,
                    self.theme.prompt_style.apply_to(prompt),
                    self.theme.success_suffix,
                    self.theme.error_style.apply_to("cancelled")
                )
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_align_and_flatten_untrusted_text() {
        let dialog = Dialog::new(
            "Confirm action",
            vec![
                Detail::new("Project", "example\nproject"),
                Detail::new("New tag", "v1.2.3"),
            ],
            "Proceed?",
        );
        assert_eq!(
            dialog.render("Preview", false),
            "Preview\n\n  Project  example project\n  New tag  v1.2.3"
        );
    }

    #[test]
    fn bypass_does_not_need_a_terminal() {
        assert_eq!(
            request(
                "action",
                true,
                &Dialog::new("Confirm action", Vec::new(), "Proceed?")
            ),
            Ok(())
        );
    }

    #[test]
    fn decline_and_escape_render_the_same_plain_cancellation() {
        let theme = ConfirmationTheme {
            theme: ColorfulTheme::default(),
            color: false,
        };
        for answer in [Some(false), None] {
            let mut rendered = String::new();
            theme
                .format_confirm_prompt_selection(&mut rendered, "Proceed?", answer)
                .unwrap();
            assert!(rendered.contains("cancelled"));
            assert!(!rendered.contains('\u{1b}'));
        }
    }
}
