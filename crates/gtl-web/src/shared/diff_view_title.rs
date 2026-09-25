//! Formats the typed title of one diff view in the displayed language.

use gtl_models::{
    diffs::{CommitIdAbbreviation, DiffViewTitle},
    settings::ViewerLanguage,
};

use super::i18n::t;

/// Names what the diff view titled `title` shows, in `language`.
pub(crate) fn diff_view_title_text(title: &DiffViewTitle, language: ViewerLanguage) -> String {
    match title {
        DiffViewTitle::Diff => t!(language, "diff-view-title-diff"),
        DiffViewTitle::MergeDiff => t!(language, "diff-view-title-merge-diff"),
        DiffViewTitle::Commit { id } => t!(
            language,
            "diff-view-title-commit",
            commit = id.abbreviated(CommitIdAbbreviation::TenCharacters),
        ),
        DiffViewTitle::Named { name } => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{diffs::CommitId, paths::ProjectName};

    use super::*;

    #[test]
    fn titles_name_the_view_in_both_languages() {
        let titles = [
            DiffViewTitle::Diff,
            DiffViewTitle::MergeDiff,
            DiffViewTitle::Commit {
                id: CommitId::try_from("abcdef0123456789abcdef0123456789abcdef01").unwrap(),
            },
            DiffViewTitle::Named {
                name: ProjectName::try_new("Release review").unwrap(),
            },
        ];
        let expected = ["diff", "merge-diff", "commit abcdef0123", "Release review"];

        for &language in ViewerLanguage::ALL {
            assert_eq!(
                titles
                    .iter()
                    .map(|title| diff_view_title_text(title, language))
                    .collect::<Vec<_>>(),
                expected,
                "{language:?}"
            );
        }
    }
}
