//! Formats the typed recipe labels that name tabs, skipped snapshots, and
//! history entries in the displayed language.

use gtl_models::{
    recipes::{RecipeLabel, RecipeLabelChanges, RecipeLabelHead},
    settings::ViewerLanguage,
};

use super::i18n::t;

/// Names the diff `label` describes in `language`.
pub(crate) fn recipe_label_text(label: &RecipeLabel, language: ViewerLanguage) -> String {
    match label {
        RecipeLabel::Named { name } => name.to_string(),
        RecipeLabel::Repository { repository } => repository.to_string(),
        RecipeLabel::Changes {
            repository,
            changes,
        } => changes_text(repository.as_str(), changes, language),
        RecipeLabel::Compared {
            repository,
            base,
            head,
        } => t!(
            language,
            "recipe-label-compared",
            repository = repository.as_str(),
            base = base.as_ref(),
            head = match head {
                RecipeLabelHead::Revision { revision } => revision.to_string(),
                RecipeLabelHead::WorkingTree => t!(language, "recipe-label-working-tree-head"),
            },
        ),
    }
}

fn changes_text(
    repository: &str,
    changes: &RecipeLabelChanges,
    language: ViewerLanguage,
) -> String {
    match changes {
        RecipeLabelChanges::Unpushed => {
            t!(language, "recipe-label-unpushed", repository = repository)
        }
        RecipeLabelChanges::UnpushedCommits { count } => t!(
            language,
            "recipe-label-unpushed-commits",
            repository = repository,
            count = count.into_inner(),
        ),
        RecipeLabelChanges::WorkingTree { base } => t!(
            language,
            "recipe-label-working-tree",
            repository = repository,
            base = base.as_ref(),
        ),
        RecipeLabelChanges::Range { range } => t!(
            language,
            "recipe-label-range",
            repository = repository,
            range = range.as_ref(),
        ),
        RecipeLabelChanges::MergeInto { base } => t!(
            language,
            "recipe-label-merge-into",
            repository = repository,
            base = base.as_ref(),
        ),
        RecipeLabelChanges::Merge { branch, upstream } => t!(
            language,
            "recipe-label-merge",
            repository = repository,
            branch = branch.to_string(),
            upstream = upstream.as_ref(),
        ),
        RecipeLabelChanges::LastCommits { count } => t!(
            language,
            "recipe-label-last-commits",
            repository = repository,
            count = count.get(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_models::{
        git::{CommitCount, GitHead, GitRange, GitRevision},
        paths::ProjectName,
    };

    use super::*;

    fn changes(changes: RecipeLabelChanges) -> RecipeLabel {
        RecipeLabel::Changes {
            repository: ProjectName::try_new("git-tools").unwrap(),
            changes,
        }
    }

    fn revision(value: &str) -> GitRevision {
        GitRevision::try_new(value).unwrap()
    }

    fn texts(language: ViewerLanguage) -> Vec<String> {
        [
            RecipeLabel::Named {
                name: ProjectName::try_new("Release review").unwrap(),
            },
            RecipeLabel::Repository {
                repository: ProjectName::try_new("git-tools").unwrap(),
            },
            changes(RecipeLabelChanges::Unpushed),
            changes(RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(0),
            }),
            changes(RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(1),
            }),
            changes(RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(3),
            }),
            changes(RecipeLabelChanges::WorkingTree {
                base: revision("v1"),
            }),
            changes(RecipeLabelChanges::Range {
                range: GitRange::try_new("v1..v2").unwrap(),
            }),
            changes(RecipeLabelChanges::MergeInto {
                base: revision("main"),
            }),
            changes(RecipeLabelChanges::Merge {
                branch: GitHead::try_from("feature".to_owned()).unwrap(),
                upstream: revision("origin/main"),
            }),
            changes(RecipeLabelChanges::LastCommits {
                count: NonZeroU32::new(1).unwrap(),
            }),
            changes(RecipeLabelChanges::LastCommits {
                count: NonZeroU32::new(2).unwrap(),
            }),
            RecipeLabel::Compared {
                repository: ProjectName::try_new("git-tools").unwrap(),
                base: revision("origin/main"),
                head: RecipeLabelHead::Revision {
                    revision: revision("feature"),
                },
            },
            RecipeLabel::Compared {
                repository: ProjectName::try_new("git-tools").unwrap(),
                base: revision("a1b2c3"),
                head: RecipeLabelHead::WorkingTree,
            },
        ]
        .iter()
        .map(|label| recipe_label_text(label, language))
        .collect()
    }

    #[test]
    fn english_labels_keep_the_established_server_wording() {
        assert_eq!(
            texts(ViewerLanguage::EnUs),
            [
                "Release review",
                "git-tools",
                "git-tools: diff",
                "git-tools: 0 commits",
                "git-tools: 1 commit",
                "git-tools: 3 commits",
                "git-tools: v1->working",
                "git-tools: v1..v2",
                "git-tools: merge ->main",
                "git-tools: merge feature->origin/main",
                "git-tools: last 1 commit",
                "git-tools: last 2 commits",
                "git-tools | origin/main->feature",
                "git-tools | a1b2c3->working",
            ]
        );
    }

    #[test]
    fn portuguese_labels_translate_counts_and_keep_git_terms() {
        assert_eq!(
            texts(ViewerLanguage::PtBr),
            [
                "Release review",
                "git-tools",
                "git-tools: diff",
                "git-tools: nenhum commit",
                "git-tools: 1 commit",
                "git-tools: 3 commits",
                "git-tools: v1->working tree",
                "git-tools: v1..v2",
                "git-tools: merge ->main",
                "git-tools: merge feature->origin/main",
                "git-tools: último commit",
                "git-tools: últimos 2 commits",
                "git-tools | origin/main->feature",
                "git-tools | a1b2c3->working tree",
            ]
        );
    }
}
