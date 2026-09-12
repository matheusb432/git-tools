use std::path::Path;

use gtl_wire::v1::{self, repository_service_client::RepositoryServiceClient};
use serial_test::serial;

use super::{ServerHarness, TestResult, live_views::git};

fn repository(path: &Path) -> TestResult {
    git(path, &["init", "-q", "-b", "main"])?;
    git(path, &["config", "user.name", "Example Author"])?;
    git(path, &["config", "user.email", "author@example.invalid"])?;
    git(path, &["config", "commit.gpgsign", "false"])?;
    git(
        path,
        &["commit", "-q", "--allow-empty", "-m", "Initial commit"],
    )
}

#[tokio::test]
#[serial(server_tracing)]
async fn pulls_one_repository_and_reports_path_and_remote_failures() -> TestResult {
    let data = tempfile::tempdir()?;
    let checkout = tempfile::tempdir()?;
    repository(checkout.path())?;
    let server = ServerHarness::start(data.path(), None).await?;
    let mut client =
        RepositoryServiceClient::with_interceptor(server.native_channel(), server.authorization());
    let request = || v1::PullRepositoryRequest {
        repository_path: checkout.path().to_string_lossy().into_owned(),
        dry_run: false,
    };

    let result = client
        .pull_repository(request())
        .await?
        .into_inner()
        .result
        .ok_or("pull result")?;
    assert_eq!(result.status(), v1::RepositorySyncStatus::Warning);
    assert_eq!(result.detail, "no 'origin' remote");
    assert_eq!(
        client
            .pull_repository(v1::PullRepositoryRequest {
                repository_path: "relative".into(),
                dry_run: false,
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(
        client
            .pull_repository(v1::PullRepositoryRequest {
                repository_path: data.path().to_string_lossy().into_owned(),
                dry_run: false,
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );

    let origin = tempfile::tempdir()?;
    git(origin.path(), &["init", "--bare", "-q"])?;
    git(
        checkout.path(),
        &["remote", "add", "origin", &origin.path().to_string_lossy()],
    )?;
    git(checkout.path(), &["push", "-q", "-u", "origin", "main"])?;
    let result = client
        .pull_repository(request())
        .await?
        .into_inner()
        .result
        .ok_or("pull result")?;
    assert_eq!(result.status(), v1::RepositorySyncStatus::UpToDate);
    assert_eq!(result.branch.as_deref(), Some("main"));
    server.stop().await?;
    Ok(())
}
