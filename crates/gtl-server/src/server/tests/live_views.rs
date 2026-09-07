use std::{path::Path, process::Command, time::Duration};

use gtl_application::live_views::save_live_view::{self, SaveLiveView};
use gtl_infra::{app_state::SqliteAppState, clock::SystemClock, git_client::HybridGitClient};
use gtl_models::live_views::LiveComparison;
use gtl_wire::{
    proto, v1,
    viewer::{ViewerActiveState, ViewerShell},
};
use tonic::{service::interceptor::InterceptedService, transport::Channel};

use super::{ServerHarness, ServerHarnessAuthorization, TestResult};

type Client = v1::viewer_service_client::ViewerServiceClient<
    InterceptedService<Channel, ServerHarnessAuthorization>,
>;

fn git(path: &Path, args: &[&str]) -> TestResult {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(())
}

async fn shell(client: &mut Client) -> TestResult<ViewerShell> {
    Ok(proto::viewer::decode_get_viewer_shell_response(
        client
            .get_viewer_shell(v1::GetViewerShellRequest {})
            .await?
            .into_inner(),
    )?)
}

async fn ready_shell(client: &mut Client) -> TestResult<ViewerShell> {
    loop {
        let shell = shell(client).await?;
        if matches!(shell.active, ViewerActiveState::Ready { .. }) {
            return Ok(shell);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn next_check(
    stream: &mut tonic::Streaming<v1::WatchViewerResponse>,
) -> TestResult<v1::ViewerLiveCheck> {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = stream.message().await? {
            if let Some(check) = event.live_check {
                return Ok(check);
            }
        }
        Err("watch ended".into())
    })
    .await?
}

#[tokio::test]
async fn live_watch_tracks_head_identity_recovers_and_catches_up_after_disconnect() -> TestResult {
    let directory = tempfile::tempdir()?;
    let repository = directory.path().join("repo");
    std::fs::create_dir(&repository)?;
    git(&repository, &["init", "-q", "-b", "main"])?;
    git(&repository, &["config", "user.name", "Live Test"])?;
    git(
        &repository,
        &["config", "user.email", "live@example.invalid"],
    )?;
    std::fs::write(repository.join("work.txt"), "base\n")?;
    git(&repository, &["add", "."])?;
    git(&repository, &["commit", "-qm", "base"])?;
    git(&repository, &["switch", "-qc", "feature"])?;
    git(&repository, &["branch", "--set-upstream-to", "main"])?;
    let database = SqliteAppState::open(directory.path())?;
    save_live_view::execute(
        SaveLiveView {
            path: repository.clone(),
            comparison: LiveComparison::LocalChanges,
        },
        &HybridGitClient,
        &mut *database.connection_lock()?,
        &SystemClock,
    )?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::with_interceptor(
        server.native_channel(),
        server.authorization(),
    );
    let initial = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let tab_id = initial.tabs[0].id;
    let request = v1::WatchViewerRequest {
        live_tab_id: Some(tab_id.into()),
    };
    let mut stream = client.watch_viewer(request).await?.into_inner();
    assert!(next_check(&mut stream).await?.error.is_none());
    let initial = shell(&mut client).await?;
    std::fs::write(repository.join("work.txt"), "uncommitted\n")?;
    assert!(next_check(&mut stream).await?.error.is_none());
    assert_eq!(shell(&mut client).await?.version, initial.version);
    git(&repository, &["commit", "--allow-empty", "-qm", "new HEAD"])?;
    assert!(next_check(&mut stream).await?.error.is_none());
    let committed = shell(&mut client).await?;
    assert!(committed.version > initial.version);
    git(&repository, &["switch", "-qc", "same-commit"])?;
    assert!(next_check(&mut stream).await?.error.is_none());
    let switched = shell(&mut client).await?;
    assert!(switched.version > committed.version);
    git(&repository, &["switch", "--detach", "-q"])?;
    assert!(next_check(&mut stream).await?.error.is_none());
    let detached = shell(&mut client).await?;
    assert!(detached.version > switched.version);
    git(
        &repository,
        &["commit", "--amend", "--allow-empty", "-qm", "amended HEAD"],
    )?;
    assert!(next_check(&mut stream).await?.error.is_none());
    let amended = shell(&mut client).await?;
    assert!(amended.version > detached.version);
    std::fs::rename(repository.join(".git"), repository.join("git-unavailable"))?;
    assert!(next_check(&mut stream).await?.error.is_some());
    assert_eq!(shell(&mut client).await?, amended);
    std::fs::rename(repository.join("git-unavailable"), repository.join(".git"))?;
    assert!(next_check(&mut stream).await?.error.is_none());
    drop(stream);
    git(
        &repository,
        &["commit", "--allow-empty", "-qm", "while disconnected"],
    )?;
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(shell(&mut client).await?.version, amended.version);
    let mut stream = client
        .watch_viewer(v1::WatchViewerRequest {
            live_tab_id: Some(tab_id.into()),
        })
        .await?
        .into_inner();
    assert!(next_check(&mut stream).await?.error.is_none());
    assert!(shell(&mut client).await?.version > amended.version);
    server.stop().await?;
    Ok(())
}
