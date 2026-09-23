use std::{path::Path, process::Command, time::Duration};

use gtl_application::live_views::save_live_view::{self, SaveLiveView};
use gtl_infra::{app_state::SqliteAppState, clock::SystemClock, git_client::HybridGitClient};
use gtl_models::live_views::LiveComparison;
use gtl_wire::{
    proto, v1,
    viewer::{ViewerActiveState, ViewerShell},
};
use tonic::transport::Channel;

use super::{ServerHarness, TestResult};

type Client = v1::viewer_service_client::ViewerServiceClient<Channel>;

pub(super) fn git(path: &Path, args: &[&str]) -> TestResult {
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

pub(super) async fn ready_shell(client: &mut Client) -> TestResult<ViewerShell> {
    loop {
        let shell = shell(client).await?;
        if matches!(&shell.active, ViewerActiveState::Ready { view }
            if view.row_source == gtl_wire::viewer::ViewerRowSourceState::Ready)
        {
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
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let initial = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let tab_id = initial.tabs[0].id;
    let request = v1::WatchViewerRequest {
        live_tab_id: Some(tab_id.into()),
        project_ids: Vec::new(),
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
            project_ids: Vec::new(),
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
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
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
    assert_eq!(full.files.len(), second.files.len());
    assert_eq!(full.commit_count, second.commit_count);
    let prepared =
        tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view: full } = prepared.active else {
        return Err("expected prepared full-context view".into());
    };
    assert_ne!(second.content_id, full.content_id);
    tokio::time::timeout(
        Duration::from_secs(10),
        assert_row_window(&mut client, &full),
    )
    .await??;
    let repeated = shell(&mut client).await?;
    assert!(
        matches!(repeated.active, ViewerActiveState::Ready { view } if view.content_id == full.content_id)
    );
    server.stop().await?;
    Ok(())
}

async fn assert_row_window(
    client: &mut Client,
    full: &gtl_wire::viewer::ViewerActiveView,
) -> TestResult {
    let mut stream = client
        .stream_viewer_rows(v1::StreamViewerRowsRequest {
            identity: Some(proto::viewer::encode_viewer_view_identity(full.identity)),
            file_id: None,
            row_range: None,
        })
        .await?
        .into_inner();
    let mut all_rows = Vec::new();
    while let Some(event) = stream.message().await? {
        let event = proto::viewer::decode_stream_viewer_rows_response(event)?;
        assert_eq!(event.identity, full.identity);
        if let gtl_wire::viewer::ViewerRowEvent::UnifiedRows { rows, .. } = event.event {
            all_rows.extend(rows);
        }
    }
    assert_eq!(all_rows.len(), full.files[0].row_count);
    assert_text_window(client, full, &all_rows[7..10]).await?;
    let mut window = client
        .stream_viewer_rows(v1::StreamViewerRowsRequest {
            identity: Some(proto::viewer::encode_viewer_view_identity(full.identity)),
            file_id: Some(full.files[0].id.as_str().to_owned()),
            row_range: Some(v1::ViewerRowRange { start: 7, count: 3 }),
        })
        .await?
        .into_inner();
    let mut window_rows = Vec::new();
    while let Some(event) = window.message().await? {
        match proto::viewer::decode_stream_viewer_rows_response(event)?.event {
            gtl_wire::viewer::ViewerRowEvent::FileStarted {
                row_count,
                start_row,
                ..
            } => {
                assert_eq!(row_count as usize, all_rows.len());
                assert_eq!(start_row, 7);
            }
            gtl_wire::viewer::ViewerRowEvent::UnifiedRows {
                start_row, rows, ..
            } => {
                assert_eq!(start_row as usize, 7 + window_rows.len());
                window_rows.extend(rows);
            }
            gtl_wire::viewer::ViewerRowEvent::FileFinished { end_row, .. } => {
                assert_eq!(end_row, 10);
            }
            event => return Err(format!("unexpected range event: {event:?}").into()),
        }
    }
    assert_eq!(window_rows, all_rows[7..10]);
    Ok(())
}

async fn assert_text_window(
    client: &mut Client,
    view: &gtl_wire::viewer::ViewerActiveView,
    expected_rows: &[gtl_wire::viewer::ViewerUnifiedRow],
) -> TestResult {
    use gtl_wire::viewer::{
        ReadViewerDiffText, ViewerDiffTextLine, ViewerRowRange, ViewerUnifiedRow,
    };
    let request = ReadViewerDiffText {
        identity: view.identity,
        file: view.files[0].id.clone(),
        row_range: ViewerRowRange::try_new(7, 3)?,
        old_side: false,
    };
    let response = client
        .read_viewer_diff_text(proto::viewer::text::encode_request(&request))
        .await?
        .into_inner();
    let lines = proto::viewer::text::decode_response(response)?;
    let expected = expected_rows
        .iter()
        .filter_map(|row| match row {
            ViewerUnifiedRow::Context(source) | ViewerUnifiedRow::Added(source) => source
                .new_line_number
                .map(|line_number| ViewerDiffTextLine {
                    line_number,
                    text: source.code.text.clone(),
                }),
            ViewerUnifiedRow::Meta(_)
            | ViewerUnifiedRow::Hunk(_)
            | ViewerUnifiedRow::Removed(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(lines, expected);
    Ok(())
}
