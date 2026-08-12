use crate::{diffs::View, shared::notes::Note};

pub(super) fn note(label: &str, view: &View) -> Option<Note> {
    view.exclusions.as_ref().map(|applied| {
        Note::info(format!(
            "{label}: {} file(s) hidden by config [diff.exclude] ({})",
            applied.hidden_paths.len(),
            applied.extensions_label(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::AppliedExclusions;

    use super::note;
    use crate::{diffs::View, testing::diffs::view};

    #[test]
    fn note_describes_the_applied_exclusion() {
        let view = View {
            exclusions: Some(AppliedExclusions {
                hidden_paths: vec!["Cargo.lock".into()],
                extensions: vec!["lock".into()],
            }),
            ..view()
        };

        assert_eq!(
            note("diff-artifact", &view).map(|note| note.text),
            Some("diff-artifact: 1 file(s) hidden by config [diff.exclude] (lock)".into())
        );
    }
}
