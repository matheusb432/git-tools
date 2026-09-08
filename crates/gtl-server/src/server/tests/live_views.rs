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

#[tokio::test]
async fn local_row_content_ids_invalidate_same_stats_edits_through_grpc() -> TestResult {
    use std::fmt::Write as _;

    let directory = tempfile::tempdir()?;
    let repository = directory.path().join("repo");
    std::fs::create_dir(&repository)?;
    git(&repository, &["init", "-q", "-b", "main"])?;
    git(&repository, &["config", "user.name", "Content Test"])?;
    git(
        &repository,
        &["config", "user.email", "content@example.invalid"],
    )?;
    let mut original = String::new();
    for index in 0..30 {
        writeln!(&mut original, "line {index}")?;
    }
    std::fs::write(repository.join("work.txt"), &original)?;
    git(&repository, &["add", "."])?;
    git(&repository, &["commit", "-qm", "base"])?;
    std::fs::write(
        repository.join("work.txt"),
        original.replace("line 15", "alpha"),
    )?;
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
    let settings = directory.path().join("settings.toml");
    std::fs::write(&settings, "")?;
    let server = ServerHarness::start(directory.path(), Some(settings)).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::with_interceptor(
        server.native_channel(),
        server.authorization(),
    );
    let first = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view: first } = first.active else {
        return Err("expected initial ready view".into());
    };
    std::fs::write(
        repository.join("work.txt"),
        original.replace("line 15", "bravo"),
    )?;
    client
        .refresh_viewer_tab(v1::RefreshViewerTabRequest {
            tab_id: first.identity.tab_id.into(),
        })
        .await?;
    let second = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view: second } = second.active else {
        return Err("expected refreshed ready view".into());
    };
    assert_eq!(first.files[0].added, second.files[0].added);
    assert_eq!(first.files[0].removed, second.files[0].removed);
    assert_ne!(first.content_id, second.content_id);
    let full = client
        .set_viewer_preference(v1::SetViewerPreferenceRequest {
            preference: Some(v1::set_viewer_preference_request::Preference::Density(
                v1::ViewerDiffDensity::Full as i32,
            )),
        })
        .await?
        .into_inner();
    let full = proto::viewer::decode_get_viewer_shell_response(v1::GetViewerShellResponse {
        shell: full.shell,
    })?;
    let ViewerActiveState::Ready { view: full } = full.active else {
        return Err("expected full-context ready view".into());
    };
    assert_ne!(second.content_id, full.content_id);
    let mut stream = client
        .stream_viewer_rows(v1::StreamViewerRowsRequest {
            identity: Some(proto::viewer::encode_viewer_view_identity(full.identity)),
            file_id: None,
        })
        .await?
        .into_inner();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = stream.message().await? {
            assert_eq!(
                proto::viewer::decode_stream_viewer_rows_response(event)?.identity,
                full.identity
            );
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })
    .await??;
    let repeated = shell(&mut client).await?;
    assert!(
        matches!(repeated.active, ViewerActiveState::Ready { view } if view.content_id == full.content_id)
    );
    server.stop().await?;
    Ok(())
}
