use gtl_application::{
    diffs::set_diff_file_reviewed, ports::DiffReviewReader, viewer::ViewerState,
};
use gtl_infra::app_state::SqliteAppState;
use gtl_models::{
    diffs::{DiffFileReviewReference, DiffReviewContentId, DiffReviewScope},
    paths::{RepositoryRelativePath, RepositoryRoot},
};
use gtl_wire::diff_review::SetDiffFileReviewed;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn review_marks_persist_per_repository_and_exact_content_and_unmark_independently() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let viewer = ViewerState::new();
    let original = DiffFileReviewReference {
        scope: DiffReviewScope::Repository(RepositoryRoot::try_new(directory.path().join("repo"))?),
        path: RepositoryRelativePath::try_new("src/a.rs".into())?,
        content_id: DiffReviewContentId::from_digest([1; 32]),
    };
    let changed = DiffFileReviewReference {
        content_id: DiffReviewContentId::from_digest([2; 32]),
        ..original.clone()
    };
    let other = DiffFileReviewReference {
        scope: DiffReviewScope::Repository(RepositoryRoot::try_new(
            directory.path().join("other"),
        )?),
        ..original.clone()
    };
    let references = [original.clone(), changed.clone(), other];
    let set = |file, reviewed| -> TestResult {
        set_diff_file_reviewed::execute(
            &SetDiffFileReviewed { file, reviewed },
            &mut *database.connection_lock()?,
            &viewer,
        )?;
        Ok(())
    };
    set(original.clone(), true)?;
    set(original.clone(), true)?;
    assert_eq!(
        database.reviewed_files(&references)?,
        [original.clone()].into_iter().collect()
    );
    set(changed.clone(), true)?;
    set(original, false)?;
    drop(database);
    let reopened = SqliteAppState::open(directory.path())?;
    assert_eq!(
        reopened.reviewed_files(&references)?,
        [changed].into_iter().collect()
    );
    Ok(())
}

#[test]
fn review_history_keeps_a_bounded_number_of_versions_per_file() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let viewer = ViewerState::new();
    let mut references = Vec::new();
    for version in 0..20 {
        let reference = DiffFileReviewReference {
            scope: DiffReviewScope::Repository(RepositoryRoot::try_new(
                directory.path().join("repo"),
            )?),
            path: RepositoryRelativePath::try_new("a.rs".into())?,
            content_id: DiffReviewContentId::from_digest([version; 32]),
        };
        set_diff_file_reviewed::execute(
            &SetDiffFileReviewed {
                file: reference.clone(),
                reviewed: true,
            },
            &mut *database.connection_lock()?,
            &viewer,
        )?;
        references.push(reference);
    }
    assert_eq!(
        database.reviewed_files(&references)?,
        references[4..].iter().cloned().collect()
    );
    Ok(())
}

#[test]
fn text_review_marks_are_shared_by_every_text_diff_and_kept_apart_from_repositories() -> TestResult
{
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let viewer = ViewerState::new();
    let text = DiffFileReviewReference {
        scope: DiffReviewScope::Text,
        path: RepositoryRelativePath::try_new("src/a.rs".into())?,
        content_id: DiffReviewContentId::from_digest([1; 32]),
    };
    let repository = DiffFileReviewReference {
        scope: DiffReviewScope::Repository(RepositoryRoot::try_new(directory.path().join("repo"))?),
        ..text.clone()
    };
    let set = |file, reviewed| -> TestResult {
        set_diff_file_reviewed::execute(
            &SetDiffFileReviewed { file, reviewed },
            &mut *database.connection_lock()?,
            &viewer,
        )?;
        Ok(())
    };

    set(text.clone(), true)?;

    assert_eq!(
        database.reviewed_files(&[text.clone(), repository.clone()])?,
        [text.clone()].into_iter().collect()
    );
    set(repository.clone(), true)?;
    set(text.clone(), false)?;
    assert_eq!(
        database.reviewed_files(&[text, repository.clone()])?,
        [repository].into_iter().collect()
    );
    Ok(())
}
