//! Field-level validation messages shared by client parsing and server rejections.

use gtl_models::{failure::Failure, settings::ViewerLanguage};

use super::failure_message::failure_message;

/// One input of a form, named after the request field the server validates.
pub(crate) trait FormField: Copy + Eq + 'static {
    /// Every input of the form.
    const ALL: &'static [Self];

    /// The request field that `Failure::InvalidRequest` names for this input.
    fn request_field(self) -> &'static str;

    /// Explains how to correct a rejected value.
    fn correction(self, language: ViewerLanguage) -> String;
}

/// Why one input was rejected, formatted in the displayed language when it renders.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FieldError {
    /// The input's own correction.
    Correction,
    /// A server failure that explains this input.
    Failure(Failure),
}

/// Validation messages for one form, keyed by its inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldErrors<F> {
    entries: Vec<(F, FieldError)>,
}

impl<F> Default for FieldErrors<F> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<F: FormField> FieldErrors<F> {
    /// Assigns an invalid-request failure to the input it names.
    ///
    /// Returns `None` when the failure concerns the whole form rather than one of its inputs.
    pub(crate) fn from_failure(failure: &Failure) -> Option<Self> {
        let Failure::InvalidRequest { field } = failure else {
            return None;
        };
        let input = F::ALL
            .iter()
            .copied()
            .find(|input| input.request_field() == field)?;
        let mut errors = Self::default();
        errors.reject(input);
        Some(errors)
    }

    /// Keeps a parsed value, or records the input's correction when parsing failed.
    pub(crate) fn parse<T, E>(&mut self, input: F, parsed: Result<T, E>) -> Option<T> {
        parsed.inspect_err(|_| self.reject(input)).ok()
    }

    /// Records the input's own correction.
    pub(crate) fn reject(&mut self, input: F) {
        self.record(input, FieldError::Correction);
    }

    /// Records `failure` as the reason `input` was rejected, replacing an earlier one.
    pub(crate) fn reject_with(&mut self, input: F, failure: Failure) {
        self.record(input, FieldError::Failure(failure));
    }

    fn record(&mut self, input: F, error: FieldError) {
        self.clear(input);
        self.entries.push((input, error));
    }

    pub(crate) fn clear(&mut self, input: F) {
        self.entries.retain(|(recorded, _)| *recorded != input);
    }

    /// Formats the rejection recorded for `input` in `language`.
    pub(crate) fn message(&self, input: F, language: ViewerLanguage) -> Option<String> {
        self.entries
            .iter()
            .find(|(recorded, _)| *recorded == input)
            .map(|(recorded, error)| match error {
                FieldError::Correction => recorded.correction(language),
                FieldError::Failure(failure) => failure_message(failure, language),
            })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ExampleField {
        Name,
        Branch,
    }

    impl FormField for ExampleField {
        const ALL: &'static [Self] = &[Self::Name, Self::Branch];

        fn request_field(self) -> &'static str {
            match self {
                Self::Name => "name",
                Self::Branch => "comparison_branch",
            }
        }

        fn correction(self, language: ViewerLanguage) -> String {
            let correction = match self {
                Self::Name => "Enter a name.",
                Self::Branch => "Enter a local branch name.",
            };
            format!("{correction} ({language})")
        }
    }

    #[test]
    fn server_rejections_reach_the_named_input_only() {
        let errors = FieldErrors::<ExampleField>::from_failure(&Failure::InvalidRequest {
            field: "comparison_branch".into(),
        })
        .unwrap();

        assert_eq!(
            errors
                .message(ExampleField::Branch, ViewerLanguage::PtBr)
                .as_deref(),
            Some("Enter a local branch name. (pt-BR)")
        );
        assert_eq!(
            errors.message(ExampleField::Name, ViewerLanguage::PtBr),
            None
        );
    }

    #[test]
    fn unknown_fields_and_other_failures_belong_to_the_form() {
        for failure in [
            Failure::InvalidRequest {
                field: "expected_revision".into(),
            },
            Failure::Changed,
        ] {
            assert_eq!(FieldErrors::<ExampleField>::from_failure(&failure), None);
        }
    }

    #[test]
    fn parsing_records_corrections_and_clearing_removes_them() {
        let mut errors = FieldErrors::default();

        assert_eq!(errors.parse(ExampleField::Name, Ok::<_, ()>(3)), Some(3));
        assert_eq!(errors.parse(ExampleField::Branch, Err::<u8, _>(())), None);
        assert_eq!(
            errors
                .message(ExampleField::Branch, ViewerLanguage::EnUs)
                .as_deref(),
            Some("Enter a local branch name. (en-US)")
        );
        errors.reject_with(ExampleField::Branch, Failure::Busy);
        assert_eq!(
            errors.message(ExampleField::Branch, ViewerLanguage::EnUs),
            Some(Failure::Busy.to_string())
        );
        errors.clear(ExampleField::Branch);
        assert!(errors.is_empty());
    }
}
