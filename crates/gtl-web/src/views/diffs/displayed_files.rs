use std::{cmp::Reverse, collections::HashSet};

use dioxus::prelude::*;
use gtl_models::settings::DiffFilesSort;
use gtl_wire::viewer::{ViewerActiveView, ViewerDiffFileId, ViewerFileSummary, ViewerRowContentId};
use sha2::{Digest, Sha256};

use super::file_filter_form::ShownFileChanges;
use crate::app::{
    application_layout::{ViewerContext, ViewerShellLoad},
    user_settings::UserSettings,
};

/// Reads the files sort, preferring an unsaved selection over the loaded shell.
pub(crate) fn use_diff_files_sort() -> Memo<DiffFilesSort> {
    let settings = try_use_context::<UserSettings>();
    let viewer = try_use_context::<ViewerContext>();
    use_memo(move || {
        settings
            .and_then(|settings| {
                settings
                    .selection
                    .read()
                    .as_ref()
                    .map(|selection| selection.diff_files_sort)
            })
            .or_else(|| {
                viewer.and_then(|viewer| match &*viewer.shell().read() {
                    ViewerShellLoad::Ready(shell) => Some(shell.preferences.diff_files_sort),
                    _ => None,
                })
            })
            .unwrap_or_default()
    })
}

/// Lists the files the text filter searches: the shown change kinds, in display order.
pub(crate) fn searchable_files(
    view: &ViewerActiveView,
    sort: DiffFilesSort,
    shown: ShownFileChanges,
    unreviewed: bool,
) -> Vec<ViewerFileSummary> {
    let mut files = view
        .files
        .iter()
        .filter(|file| {
            shown.shows_status(file.status)
                && (!unreviewed || !file.review.as_ref().is_some_and(|review| review.reviewed))
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_files(&mut files, sort);
    files
}

/// Projects the files the workspace shows, in display order.
///
/// Changing the server's file list or order derives a distinct content identity.
pub(crate) fn displayed_view(
    view: &ViewerActiveView,
    searchable: &[ViewerFileSummary],
    text_matches: Option<&HashSet<ViewerDiffFileId>>,
) -> ViewerActiveView {
    let files = searchable
        .iter()
        .filter(|file| text_matches.is_none_or(|matches| matches.contains(&file.id)))
        .cloned()
        .collect::<Vec<_>>();
    let content_id = if same_file_order(&files, &view.files) {
        view.content_id
    } else {
        displayed_content_id(view.content_id, &files)
    };
    ViewerActiveView {
        files,
        content_id,
        ..view.clone()
    }
}

fn sort_files(files: &mut [ViewerFileSummary], sort: DiffFilesSort) {
    match sort {
        DiffFilesSort::Path => {}
        DiffFilesSort::Changes => {
            files.sort_by_key(|file| Reverse(file.added.saturating_add(file.removed)));
        }
    }
}

fn same_file_order(left: &[ViewerFileSummary], right: &[ViewerFileSummary]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.id == right.id)
}

fn displayed_content_id(
    source: ViewerRowContentId,
    files: &[ViewerFileSummary],
) -> ViewerRowContentId {
    let mut digest = Sha256::new();
    digest.update(b"gtl.viewer.displayed-files.v1\0");
    digest.update(source.into_digest());
    digest.update((files.len() as u64).to_be_bytes());
    for file in files {
        let id = file.id.as_str().as_bytes();
        digest.update((id.len() as u64).to_be_bytes());
        digest.update(id);
    }
    ViewerRowContentId::from_digest(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerFileStatus;

    use super::*;
    use crate::{
        test_support::{TestResult, viewer_active_view, viewer_file_summary, viewer_tab_id},
        views::diffs::file_filter_form::{FileChangeKind, FileFilterEdit, FileFilterForm},
    };

    fn view() -> TestResult<ViewerActiveView> {
        let mut view = viewer_active_view(viewer_tab_id(1)?)?;
        view.files = vec![
            viewer_file_summary(0, "src/a.rs", ViewerFileStatus::Modified, 1, 1)?,
            viewer_file_summary(1, "src/b.rs", ViewerFileStatus::Added, 40, 0)?,
            viewer_file_summary(2, "src/c.rs", ViewerFileStatus::Deleted, 0, 2)?,
            viewer_file_summary(3, "src/d.rs", ViewerFileStatus::Renamed, 30, 10)?,
        ];
        Ok(view)
    }

    fn paths(view: &ViewerActiveView) -> Vec<String> {
        view.files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect()
    }

    fn displayed(
        view: &ViewerActiveView,
        sort: DiffFilesSort,
        shown: ShownFileChanges,
        text_matches: Option<&HashSet<ViewerDiffFileId>>,
    ) -> ViewerActiveView {
        displayed_view(
            view,
            &searchable_files(view, sort, shown, false),
            text_matches,
        )
    }

    #[test]
    fn unfiltered_path_order_keeps_the_server_view_and_its_content_identity() -> TestResult {
        let view = view()?;

        let displayed = displayed(
            &view,
            DiffFilesSort::Path,
            ShownFileChanges::default(),
            None,
        );

        assert_eq!(displayed, view);
        Ok(())
    }

    #[test]
    fn unreviewed_filter_removes_only_reviewed_files_and_composes_with_change_kinds() -> TestResult
    {
        use gtl_models::{diffs::DiffReviewContentId, paths::RepositoryRoot};
        use gtl_wire::diff_review::{DiffFileReview, DiffFileReviewReference};

        let mut view = view()?;
        view.files[0].review = Some(DiffFileReview {
            reference: DiffFileReviewReference {
                repository: RepositoryRoot::try_new(
                    crate::test_support::absolute_file_path("/repo")?
                        .as_path()
                        .to_path_buf(),
                )?,
                path: view.files[0].path.clone(),
                content_id: DiffReviewContentId::from_digest([1; 32]),
            },
            reviewed: true,
        });
        let all = searchable_files(
            &view,
            DiffFilesSort::Path,
            ShownFileChanges::default(),
            false,
        );
        assert_eq!(all.len(), 4);
        let filtered = searchable_files(
            &view,
            DiffFilesSort::Path,
            ShownFileChanges::default(),
            true,
        );
        assert_eq!(
            filtered
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            view.files[1..]
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>()
        );

        let mut form = FileFilterForm::default();
        FileFilterEdit::Toggle(FileChangeKind::Added).apply(&mut form);
        let filtered = searchable_files(&view, DiffFilesSort::Changes, form.shown, true);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].path, view.files[3].path);
        Ok(())
    }

    #[test]
    fn changes_order_puts_most_changed_lines_first_and_keeps_path_order_for_ties() -> TestResult {
        let view = view()?;

        let displayed = displayed(
            &view,
            DiffFilesSort::Changes,
            ShownFileChanges::default(),
            None,
        );

        assert_eq!(
            paths(&displayed),
            ["src/b.rs", "src/d.rs", "src/a.rs", "src/c.rs"]
        );
        assert_ne!(displayed.content_id, view.content_id);
        assert_eq!(
            displayed_view(
                &view,
                &searchable_files(
                    &view,
                    DiffFilesSort::Changes,
                    ShownFileChanges::default(),
                    false
                ),
                None,
            )
            .content_id,
            displayed.content_id
        );
        Ok(())
    }

    #[test]
    fn status_and_text_filters_keep_only_files_that_pass_both() -> TestResult {
        let view = view()?;
        let mut form = FileFilterForm::default();
        FileFilterEdit::Toggle(FileChangeKind::Added).apply(&mut form);
        let matches = HashSet::from([
            ViewerDiffFileId::for_index(1),
            ViewerDiffFileId::for_index(3),
        ]);

        let displayed = displayed(&view, DiffFilesSort::Path, form.shown, Some(&matches));

        assert_eq!(paths(&displayed), ["src/d.rs"]);
        assert_ne!(displayed.content_id, view.content_id);
        Ok(())
    }
}
