use gtl_infra::testing::TestRepository;
use gtl_wire::v1::{self, repository_service_client::RepositoryServiceClient};
use serial_test::serial;

use super::{ServerHarness, TestResult};

#[tokio::test]
#[serial(server_tracing)]
async fn pulls_one_repository_and_reports_path_and_remote_failures() -> TestResult {
    let data = tempfile::tempdir()?;
    let checkout = TestRepository::new();
    checkout.commit_all("Initial commit");
    let server = ServerHarness::start(data.path(), None).await?;
    let mut client = RepositoryServiceClient::new(server.native_channel());
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
    checkout.add_bare_origin(origin.path());
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
