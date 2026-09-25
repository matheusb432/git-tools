use gtl_models::diffs::ExtensionFilterMode;

use crate::{diffs::View, shared::notes::Note};

pub(super) fn note(label: &str, view: &View) -> Option<Note> {
    view.extension_filter.as_ref().map(|applied| {
        let rule = match applied.filter.mode() {
            ExtensionFilterMode::Hide => "hide",
            ExtensionFilterMode::Only => "show only",
        };
        Note::info(format!(
            "{label}: {} file(s) hidden by the saved extension filter ({rule} {})",
            applied.hidden_paths.len(),
            applied.extensions_label(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::{
        AppliedExtensionFilter, ExtensionFilter, ExtensionFilterMode, FileExtensions,
    };

    use super::note;
    use crate::{diffs::View, utils::diffs::view};

    fn applied(mode: ExtensionFilterMode, extensions: &[&str], path: &str) -> View {
        View {
            extension_filter: Some(AppliedExtensionFilter {
                hidden_paths: vec![crate::utils::repository_relative_path(path)],
                filter: ExtensionFilter::new(mode, FileExtensions::new(extensions)),
            }),
            ..view()
        }
    }

    #[test]
    fn note_describes_the_applied_filter_rule() {
        assert_eq!(
            note(
                "diff-artifact",
                &applied(ExtensionFilterMode::Hide, &["lock"], "Cargo.lock")
            )
            .map(|note| note.text),
            Some(
                "diff-artifact: 1 file(s) hidden by the saved extension filter (hide lock)".into()
            )
        );
        assert_eq!(
            note(
                "diff-artifact",
                &applied(ExtensionFilterMode::Only, &["md", "rs"], "Cargo.lock")
            )
            .map(|note| note.text),
            Some(
                "diff-artifact: 1 file(s) hidden by the saved extension filter (show only md, rs)"
                    .into()
            )
        );
    }
}
