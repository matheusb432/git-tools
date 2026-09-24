//! Field-level validation messages shared by client parsing and server rejections.

use gtl_models::failure::Failure;

/// One input of a form, named after the request field the server validates.
pub(crate) trait FormField: Copy + Eq + 'static {
    #[cfg_attr(
        all(not(feature = "desktop"), not(test)),
        expect(
            dead_code,
            reason = "component previews show errors without parsing requests"
        )
    )]
    /// Every input of the form.
    const ALL: &'static [Self];

    /// The request field that `Failure::InvalidRequest` names for this input.
    fn request_field(self) -> &'static str;

    /// Explains how to correct a rejected value.
    fn correction(self) -> &'static str;
}

/// Validation messages for one form, keyed by its inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldErrors<F> {
    entries: Vec<(F, String)>,
}

impl<F> Default for FieldErrors<F> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<F: FormField> FieldErrors<F> {
    #[cfg_attr(
        all(not(feature = "desktop"), not(test)),
        expect(
            dead_code,
            reason = "component previews show errors without parsing requests"
        )
    )]
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

    #[cfg_attr(
        all(not(feature = "desktop"), not(test)),
        expect(
            dead_code,
            reason = "component previews show errors without parsing requests"
        )
    )]
    /// Keeps a parsed value, or records the input's correction when parsing failed.
    pub(crate) fn parse<T, E>(&mut self, input: F, parsed: Result<T, E>) -> Option<T> {
        parsed.inspect_err(|_| self.reject(input)).ok()
    }

    /// Records the input's own correction.
    pub(crate) fn reject(&mut self, input: F) {
        self.insert(input, input.correction());
    }

    /// Records `message` for `input`, replacing an earlier one.
    pub(crate) fn insert(&mut self, input: F, message: impl Into<String>) {
        self.clear(input);
        self.entries.push((input, message.into()));
    }

    pub(crate) fn clear(&mut self, input: F) {
        self.entries.retain(|(recorded, _)| *recorded != input);
    }

    pub(crate) fn message(&self, input: F) -> Option<String> {
        self.entries
            .iter()
            .find(|(recorded, _)| *recorded == input)
            .map(|(_, message)| message.clone())
    }

    #[cfg_attr(
        all(not(feature = "desktop"), not(test)),
        expect(
            dead_code,
            reason = "component previews show errors without parsing requests"
        )
    )]
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

        fn correction(self) -> &'static str {
            match self {
                Self::Name => "Enter a name.",
                Self::Branch => "Enter a local branch name.",
            }
        }
    }

    #[test]
    fn server_rejections_reach_the_named_input_only() {
        let errors = FieldErrors::<ExampleField>::from_failure(&Failure::InvalidRequest {
            field: "comparison_branch".into(),
        })
        .unwrap();

        assert_eq!(
            errors.message(ExampleField::Branch).as_deref(),
            Some("Enter a local branch name.")
        );
        assert_eq!(errors.message(ExampleField::Name), None);
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
            errors.message(ExampleField::Branch).as_deref(),
            Some("Enter a local branch name.")
        );
        errors.insert(ExampleField::Branch, "Branch is missing.");
        assert_eq!(
            errors.message(ExampleField::Branch).as_deref(),
            Some("Branch is missing.")
        );
        errors.clear(ExampleField::Branch);
        assert!(errors.is_empty());
    }
}
