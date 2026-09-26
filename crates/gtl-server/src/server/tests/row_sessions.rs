use std::{fmt::Write as _, time::Duration};

use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_local_transport::LocalEndpoint;
use gtl_wire::{
    proto, v1,
    viewer::{
        StreamViewerRows, ViewerActiveState, ViewerActiveView, ViewerRowEvent, ViewerRowRange,
    },
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

use super::{ServerHarness, TestResult, live_views::ready_shell};

type Client = v1::viewer_service_client::ViewerServiceClient<Channel>;

async fn fixture() -> TestResult<(tempfile::TempDir, ServerHarness, Client, ViewerActiveView)> {
    fixture_tabs(1).await
}

async fn fixture_tabs(
    tabs: usize,
) -> TestResult<(tempfile::TempDir, ServerHarness, Client, ViewerActiveView)> {
    let mut source = String::new();
    for row in 0..2_000 {
        writeln!(&mut source, "row {row}")?;
    }
    fixture_source(tabs, &source).await
}

async fn fixture_source(
    tabs: usize,
    source: &str,
) -> TestResult<(tempfile::TempDir, ServerHarness, Client, ViewerActiveView)> {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    for tab in 0..tabs {
        let repository = TestRepository::init(directory.path().join(format!("repo-{tab}")));
        repository.write("work.txt", "base\n");
        repository.commit_all("base");
        repository.write("work.txt", source);
        super::seed_live_tabs(&database, [super::working_tree_recipe(repository.root())])?;
    }
    let settings = directory.path().join("settings.toml");
    std::fs::write(&settings, "")?;
    let server = ServerHarness::start(directory.path(), Some(settings)).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view } = shell.active else {
        return Err("expected a ready fixture".into());
    };
    Ok((directory, server, client, *view))
}

fn demand(view: &ViewerActiveView, start: u32, count: u32) -> TestResult<StreamViewerRows> {
    Ok(StreamViewerRows {
        identity: view.identity,
        file: Some(view.files[0].id.clone()),
        row_range: Some(ViewerRowRange::try_new(start, count)?),
    })
}

fn frame(
    view: &ViewerActiveView,
    request_id: u64,
    start: u32,
) -> TestResult<v1::StreamViewerRowSessionRequest> {
    Ok(v1::StreamViewerRowSessionRequest {
        request_id,
        rows: Some(proto::viewer::encode_stream_viewer_rows_request(demand(
            view, start, 512,
        )?)),
    })
}

async fn open(
    client: &mut Client,
    view: &ViewerActiveView,
) -> TestResult<(
    mpsc::Sender<v1::StreamViewerRowSessionRequest>,
    tonic::Streaming<v1::StreamViewerRowSessionResponse>,
)> {
    let (sender, receiver) = mpsc::channel(1);
    sender.send(frame(view, 1, 0)?).await?;
    let stream = client
        .stream_viewer_row_session(ReceiverStream::new(receiver))
        .await?
        .into_inner();
    Ok((sender, stream))
}

async fn collect(
    stream: &mut tonic::Streaming<v1::StreamViewerRowSessionResponse>,
    request_id: u64,
) -> TestResult<usize> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut count = 0;
        let mut sequence = 0;
        while let Some(response) = stream.message().await? {
            if response.request_id < request_id {
                continue;
            }
            assert_eq!(response.request_id, request_id);
            match response.event.ok_or("missing session event")? {
                v1::stream_viewer_row_session_response::Event::Rows(row) => {
                    assert_eq!(row.sequence, sequence);
                    sequence += 1;
                    count +=
                        row_count(proto::viewer::decode_stream_viewer_rows_response(row)?.event);
                }
                v1::stream_viewer_row_session_response::Event::Completed(_) => return Ok(count),
                v1::stream_viewer_row_session_response::Event::Failed(error) => {
                    return Err(format!("row session failed: {error:?}").into());
                }
            }
        }
        Err("row session ended before completion".into())
    })
    .await?
}

#[tokio::test]
async fn long_lines_cross_grpc_as_previews_but_copy_reads_complete_source() -> TestResult {
    use gtl_wire::viewer::{ReadViewerDiffText, ViewerUnifiedRow};
    use prost::Message as _;

    let source = "a".repeat(94_718);
    let (_directory, server, mut client, view) = fixture_source(1, &format!("{source}\n")).await?;
    let mut stream = client
        .stream_viewer_rows(v1::StreamViewerRowsRequest {
            identity: Some(proto::viewer::encode_viewer_view_identity(view.identity)),
            file_id: Some(view.files[0].id.as_str().to_owned()),
            row_range: None,
        })
        .await?
        .into_inner();
    let mut received = Vec::new();
    let mut row_count = 0;
    while let Some(response) = stream.message().await? {
        assert!(response.encoded_len() < 2_000);
        match proto::viewer::decode_stream_viewer_rows_response(response)?.event {
            ViewerRowEvent::FileStarted {
                row_count: count, ..
            } => row_count = count,
            ViewerRowEvent::UnifiedRows { rows, .. } => received.extend(rows),
            ViewerRowEvent::FileFinished { .. } => {}
            event => return Err(format!("unexpected row event: {event:?}").into()),
        }
    }
    let added = received
        .into_iter()
        .filter_map(|row| match row {
            ViewerUnifiedRow::Added(row) => Some(row.code),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].text, "a".repeat(500));
    assert_eq!(added[0].omitted_character_count, Some(94_218));
    let request = ReadViewerDiffText {
        identity: view.identity,
        file: view.files[0].id.clone(),
        row_range: ViewerRowRange::try_new(0, row_count)?,
        old_side: false,
    };
    let response = client
        .read_viewer_diff_text(proto::viewer::text::encode_request(&request))
        .await?
        .into_inner();
    let lines = proto::viewer::text::decode_response(response)?;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, source);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn native_windows_reuse_one_session_and_release_it_on_disconnect() -> TestResult {
    let (directory, server, mut client, view) = fixture().await?;
    let endpoint = LocalEndpoint::from_root(directory.path())?;
    let mut native = gtl_client::ViewerClient::connect(&endpoint).await?;
    for index in 0..25 {
        let count = receive_native(&mut native, demand(&view, index * 16, 512)?).await?;
        assert_eq!(count, 512);
    }
    let mut held = Vec::new();
    for _ in 0..9 {
        held.push(open(&mut client, &view).await?);
    }
    let (sender, receiver) = mpsc::channel(1);
    sender.send(frame(&view, 1, 0)?).await?;
    let error = client
        .stream_viewer_row_session(ReceiverStream::new(receiver))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    drop(native);
    let replacement =
        tokio::time::timeout(Duration::from_secs(5), reopen(&mut client, &view)).await??;
    drop(replacement);
    drop(held);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn row_sessions_supersede_demand_recover_from_range_errors_and_close_with_the_tab()
-> TestResult {
    let (_directory, server, mut client, view) = fixture().await?;
    let (sender, mut stream) = open(&mut client, &view).await?;
    sender.send(frame(&view, 2, 512)?).await?;
    assert_eq!(collect(&mut stream, 2).await?, 512);
    sender.send(frame(&view, 3, 10_000)?).await?;
    let failure = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await??
        .ok_or("missing range failure")?;
    assert!(matches!(
        failure.event,
        Some(v1::stream_viewer_row_session_response::Event::Failed(_))
    ));
    sender.send(frame(&view, 4, 0)?).await?;
    assert_eq!(collect(&mut stream, 4).await?, 512);
    client
        .close_viewer_tab(v1::CloseViewerTabRequest {
            tab_id: view.identity.tab_id.into(),
        })
        .await?;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await??
            .is_none()
    );
    drop(sender);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn native_session_eviction_preserves_tabs_and_reopens_their_streams() -> TestResult {
    let (directory, server, mut client, _) = fixture_tabs(12).await?;
    let endpoint = LocalEndpoint::from_root(directory.path())?;
    let mut native = gtl_client::ViewerClient::connect(&endpoint).await?;
    let shell = native.get_shell().await?;
    assert_eq!(shell.tabs.len(), 12);
    for tab in shell.tabs.iter().chain(shell.tabs.iter().take(2)) {
        client
            .activate_viewer_tab(v1::ActivateViewerTabRequest {
                tab_id: tab.id.into(),
            })
            .await?;
        let shell =
            tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
        let ViewerActiveState::Ready { view } = shell.active else {
            return Err("expected active tab".into());
        };
        assert_eq!(view.identity.tab_id, tab.id);
        assert_eq!(
            receive_native(&mut native, demand(&view, 0, 64)?).await?,
            64
        );
    }
    assert_eq!(native.get_shell().await?.tabs.len(), 12);
    drop(native);
    server.stop().await?;
    Ok(())
}

fn row_count(event: ViewerRowEvent) -> usize {
    match event {
        ViewerRowEvent::UnifiedRows { rows, .. } => {
            assert!(rows.len() <= 64);
            rows.len()
        }
        _ => 0,
    }
}

async fn reopen(
    client: &mut Client,
    view: &ViewerActiveView,
) -> TestResult<(
    mpsc::Sender<v1::StreamViewerRowSessionRequest>,
    tonic::Streaming<v1::StreamViewerRowSessionResponse>,
)> {
    loop {
        if let Ok(replacement) = open(client, view).await {
            return Ok(replacement);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn receive_native(
    native: &mut gtl_client::ViewerClient,
    request: StreamViewerRows,
) -> TestResult<usize> {
    for _ in 0..5 {
        let mut stream = native.stream_rows(request.clone()).await?;
        let first = stream.message().await;
        if matches!(
            first,
            Err(gtl_client::ViewerClientError::Failed(
                gtl_models::failure::Failure::Busy
            ))
        ) {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        }
        let mut count = row_count(first?.ok_or("missing native rows")?.event);
        while let Some(row) = stream.message().await? {
            count += row_count(row.event);
        }
        return Ok(count);
    }
    Err("stream eviction did not release capacity".into())
}
