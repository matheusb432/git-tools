use gtl_application::{
    diffs::{
        remote_diff::{GitHubApiResponse, StoredRemoteDiff},
        resolve_remote_diff::{self, RemoteDiffResolution, ResolveRemoteDiff},
        store_diff_text,
        store_remote_diff::{self, StoreRemoteDiff, StoreRemoteDiffError},
    },
    ports::DiffTextReader,
};
use gtl_infra::app_state::SqliteAppState;
use gtl_models::{
    diffs::DiffText,
    failure::{DiffTextFailure, RemoteDiffFailure},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const CACHED_REMOTE_DIFFS_MAX: usize = 64;
const ORIGIN: &str = "https://github.com/example-org/widget";

fn patch(path: &str) -> String {
    format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n")
}

fn range(head: usize) -> String {
    format!("v1.0.0...v1.{head}.0")
}

fn resolve(
    database: &SqliteAppState,
    origin: &str,
    head: usize,
    refresh: bool,
) -> Result<RemoteDiffResolution, Box<dyn std::error::Error>> {
    Ok(resolve_remote_diff::execute(
        &ResolveRemoteDiff {
            origin: origin.to_owned(),
            range: range(head),
            refresh,
        },
        &mut *database.connection_lock()?,
    )?)
}

fn store(
    connection: &mut rusqlite::Connection,
    head: usize,
    status: u16,
    body: &str,
) -> Result<StoredRemoteDiff, StoreRemoteDiffError> {
    store_remote_diff::execute(
        StoreRemoteDiff {
            origin: ORIGIN.to_owned(),
            range: range(head),
            response: GitHubApiResponse {
                status,
                rate_limit_remaining: Some(4999),
                body: body.as_bytes().to_vec(),
            },
        },
        connection,
    )
}

fn is_cached(database: &SqliteAppState, head: usize) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(matches!(
        resolve(database, ORIGIN, head, false)?,
        RemoteDiffResolution::Stored(_)
    ))
}

#[test]
fn a_stored_diff_answers_equivalent_origins_until_refreshed() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;

    let RemoteDiffResolution::Fetch(request) = resolve(&database, ORIGIN, 1, false)? else {
        return Err("an uncached diff must be fetched".into());
    };
    assert_eq!(
        request.path,
        "repos/example-org/widget/compare/v1.0.0...v1.1.0"
    );
    let stored = store(
        &mut *database.connection_lock()?,
        1,
        200,
        &patch("src/widget.rs"),
    )?;
    drop(database);
    let database = SqliteAppState::open(directory.path())?;

    assert_eq!(
        resolve(&database, "git@github.com:Example-Org/Widget.git", 1, false)?,
        RemoteDiffResolution::Stored(stored.clone())
    );
    assert_eq!(stored.label.as_ref(), "example-org/widget v1.0.0...v1.1.0");
    assert_eq!(
        database.diff_text(&stored.id)?,
        Some(DiffText::try_new(patch("src/widget.rs"))?)
    );
    assert!(matches!(
        resolve(&database, ORIGIN, 1, true)?,
        RemoteDiffResolution::Fetch(_)
    ));
    Ok(())
}

#[test]
fn refusals_and_invalid_text_are_not_cached() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;

    let not_found = store(
        &mut *database.connection_lock()?,
        1,
        404,
        r#"{"message":"Not Found"}"#,
    )
    .unwrap_err();
    let empty = store(&mut *database.connection_lock()?, 1, 200, "").unwrap_err();

    assert!(matches!(
        not_found,
        StoreRemoteDiffError::Remote(RemoteDiffFailure::NotFound)
    ));
    assert!(matches!(
        empty,
        StoreRemoteDiffError::Invalid(DiffTextFailure::NoFiles)
    ));
    assert!(!is_cached(&database, 1)?);
    Ok(())
}

#[test]
fn the_least_recently_used_diff_is_evicted_beyond_the_cap() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    for head in 0..CACHED_REMOTE_DIFFS_MAX {
        store(
            &mut *database.connection_lock()?,
            head,
            200,
            &patch(&format!("src/{head}.rs")),
        )?;
    }
    resolve(&database, ORIGIN, 0, false)?;

    store(
        &mut *database.connection_lock()?,
        CACHED_REMOTE_DIFFS_MAX,
        200,
        &patch("src/newest.rs"),
    )?;

    assert!(is_cached(&database, 0)?);
    assert!(!is_cached(&database, 1)?);
    assert!(is_cached(&database, 2)?);
    Ok(())
}

#[test]
fn cached_texts_survive_pruning_of_unreferenced_texts() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let cached = store(
        &mut *database.connection_lock()?,
        1,
        200,
        &patch("src/cached.rs"),
    )?;

    for index in 0..32 {
        store_diff_text::execute(
            &DiffText::try_new(patch(&format!("src/{index}.rs")))?,
            &mut *database.connection_lock()?,
        )?;
    }

    assert!(database.diff_text(&cached.id)?.is_some());
    assert!(is_cached(&database, 1)?);
    Ok(())
}
