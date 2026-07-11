/// Answer selected when the user submits an empty confirmation reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefaultAnswer {
    Yes,
    // Kept available for destructive commands that should require an explicit yes.
    #[allow(dead_code)]
    No,
}

impl DefaultAnswer {
    const fn hint(self) -> &'static str {
        match self {
            Self::Yes => "[Y/n]",
            Self::No => "[y/N]",
        }
    }
}

/// A recognized confirmation answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Yes,
    No,
}

/// Why a confirmation reply could not be interpreted as yes or no.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum AnswerError {
    #[error("unrecognized answer {0:?} (expected y/yes or n/no)")]
    Invalid(String),
}

/// Complete result of applying confirmation policy to an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Confirmation {
    Proceed,
    Declined,
    RefuseNonInteractive,
    Invalid(AnswerError),
}

/// Terminal and input seam used by confirmation policy.
pub(crate) trait Confirm {
    fn interactive(&self) -> bool;

    fn confirm(&self, question: &str, default: DefaultAnswer) -> Result<Answer, AnswerError>;
}

/// Real confirmation adapter backed by the process terminal and standard input.
pub(crate) struct RealConfirm;

impl Confirm for RealConfirm {
    fn interactive(&self) -> bool {
        stdin_is_terminal()
    }

    fn confirm(&self, question: &str, default: DefaultAnswer) -> Result<Answer, AnswerError> {
        use std::io::Write;

        print!("{question} {} ", default.hint());
        let _ = std::io::stdout().flush();

        let mut input = String::new();
        let read = std::io::stdin().read_line(&mut input).map(|_| input);
        answer_from_input(read, default)
    }
}

/// Reports whether standard input can safely host an interactive prompt.
pub(crate) fn stdin_is_terminal() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdin())
}

/// Applies bypass, terminal, and answer policy for one confirmation request.
pub(crate) fn request(
    confirm: &impl Confirm,
    bypass: bool,
    question: &str,
    default: DefaultAnswer,
) -> Confirmation {
    if bypass {
        return Confirmation::Proceed;
    }
    if !confirm.interactive() {
        return Confirmation::RefuseNonInteractive;
    }

    match confirm.confirm(question, default) {
        Ok(Answer::Yes) => Confirmation::Proceed,
        Ok(Answer::No) => Confirmation::Declined,
        Err(error) => Confirmation::Invalid(error),
    }
}

fn interpret(input: &str, default: DefaultAnswer) -> Result<Answer, AnswerError> {
    let trimmed = input.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "" => Ok(match default {
            DefaultAnswer::Yes => Answer::Yes,
            DefaultAnswer::No => Answer::No,
        }),
        "y" | "yes" => Ok(Answer::Yes),
        "n" | "no" => Ok(Answer::No),
        _ => Err(AnswerError::Invalid(trimmed.to_string())),
    }
}

fn answer_from_input(
    input: std::io::Result<String>,
    default: DefaultAnswer,
) -> Result<Answer, AnswerError> {
    match input {
        Ok(input) => interpret(&input, default),
        Err(_) => Ok(Answer::No),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestConfirm {
        interactive: bool,
        answer: Result<Answer, AnswerError>,
    }

    impl Confirm for TestConfirm {
        fn interactive(&self) -> bool {
            self.interactive
        }

        fn confirm(&self, _question: &str, _default: DefaultAnswer) -> Result<Answer, AnswerError> {
            self.answer.clone()
        }
    }

    #[test]
    fn empty_answer_uses_the_configured_default() {
        assert_eq!(interpret("", DefaultAnswer::Yes), Ok(Answer::Yes));
        assert_eq!(interpret(" \n", DefaultAnswer::No), Ok(Answer::No));
    }

    #[test]
    fn explicit_answers_override_either_default() {
        for input in ["y", "Y", "yes", " YES "] {
            assert_eq!(interpret(input, DefaultAnswer::No), Ok(Answer::Yes));
        }
        for input in ["n", "N", "no", " NO "] {
            assert_eq!(interpret(input, DefaultAnswer::Yes), Ok(Answer::No));
        }
    }

    #[test]
    fn unrecognized_answer_is_an_error() {
        assert_eq!(
            interpret(" maybe ", DefaultAnswer::Yes),
            Err(AnswerError::Invalid("maybe".to_string()))
        );
    }

    #[test]
    fn prompt_hint_reflects_the_configured_default() {
        assert_eq!(DefaultAnswer::Yes.hint(), "[Y/n]");
        assert_eq!(DefaultAnswer::No.hint(), "[y/N]");
    }

    #[test]
    fn input_read_failure_declines_safely() {
        let read = Err(std::io::Error::other("closed stdin"));

        assert_eq!(answer_from_input(read, DefaultAnswer::Yes), Ok(Answer::No));
    }

    #[test]
    fn bypass_proceeds_without_an_interactive_terminal() {
        let confirm = TestConfirm {
            interactive: false,
            answer: Err(AnswerError::Invalid("unused".to_string())),
        };

        assert_eq!(
            request(&confirm, true, "Proceed?", DefaultAnswer::Yes),
            Confirmation::Proceed
        );
    }

    #[test]
    fn non_interactive_request_refuses() {
        let confirm = TestConfirm {
            interactive: false,
            answer: Ok(Answer::Yes),
        };

        assert_eq!(
            request(&confirm, false, "Proceed?", DefaultAnswer::Yes),
            Confirmation::RefuseNonInteractive
        );
    }

    #[test]
    fn interactive_request_maps_yes_and_no_answers() {
        let yes = TestConfirm {
            interactive: true,
            answer: Ok(Answer::Yes),
        };
        let no = TestConfirm {
            interactive: true,
            answer: Ok(Answer::No),
        };

        assert_eq!(
            request(&yes, false, "Proceed?", DefaultAnswer::Yes),
            Confirmation::Proceed
        );
        assert_eq!(
            request(&no, false, "Proceed?", DefaultAnswer::Yes),
            Confirmation::Declined
        );
    }

    #[test]
    fn interactive_request_preserves_invalid_answer() {
        let error = AnswerError::Invalid("maybe".to_string());
        let confirm = TestConfirm {
            interactive: true,
            answer: Err(error.clone()),
        };

        assert_eq!(
            request(&confirm, false, "Proceed?", DefaultAnswer::Yes),
            Confirmation::Invalid(error)
        );
    }
}
