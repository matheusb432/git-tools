use std::time::Duration;

use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_wire::{
    diff_review::SetDiffFileReviewed,
    proto,
    terminal_diff::TerminalDiff,
    v1,
    viewer::{ViewerActiveState, ViewerActiveView},
};
use tonic::transport::Channel;

use super::{ServerHarness, TestResult, live_views::ready_shell};

type Client = v1::viewer_service_client::ViewerServiceClient<Channel>;

async fn active(client: &mut Client) -> TestResult<ViewerActiveView> {
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(client)).await??;
    let ViewerActiveState::Ready { view } = shell.active else {
        return Err("expected a ready diff".into());
    };
    Ok(*view)
}

async fn terminal(server: &ServerHarness, repository: &TestRepository) -> TestResult<TerminalDiff> {
    let mut client =
        v1::terminal_diff_service_client::TerminalDiffServiceClient::new(server.native_channel());
    let mut stream = client
        .read_terminal_diff(v1::ReadTerminalDiffRequest {
            working_directory: repository.path().to_string_lossy().into_owned(),
            target: Some(v1::DiffTarget {
                selection: Some(v1::diff_target::Selection::BaseRevision("main".into())),
            }),
            text: None,
        })
        .await?
        .into_inner();
    let mut decoder = proto::terminal_diff::SnapshotDecoder::default();
    while let Some(event) = stream.message().await? {
        decoder.accept(event)?;
    }
    Ok(decoder.finish()?)
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one journey checks shared versions through refresh and server restart"
)]
async fn diff_reviews_share_exact_versions_across_frontends_refresh_and_restart() -> TestResult {
    let directory = tempfile::tempdir()?;
    let repository = TestRepository::init(std::fs::canonicalize(directory.path())?.join("repo"));
    repository.write("a.rs", "first\n");
    repository.write("b.rs", "first\n");
    repository.write("binary.bin", b"\0first");
    repository.commit_all("base");
    repository.git(&["switch", "-qc", "feature"]);
    repository.write("a.rs", "reviewed a\n");
    repository.write("b.rs", "reviewed b\n");
    repository.write("binary.bin", b"\0reviewed");
    let data = directory.path().join("data");
    let database = SqliteAppState::open(&data)?;
    let mut recipe = super::working_tree_recipe(repository.root());
    if let gtl_application::recipes::RecipeSource::LocalRepo { op, .. } = &mut recipe.source {
        *op = gtl_application::recipes::RecipeOp::Diff {
            target: gtl_application::recipes::RecipeTarget::Base {
                rev: gtl_models::git::GitRevision::main(),
            },
        };
    }
    super::seed_live_tabs(&database, [recipe])?;
    let server = ServerHarness::start(&data, Some(data.join("settings.toml"))).await?;
    let mut client = Client::new(server.native_channel());
    let initial = active(&mut client).await?;
    assert_eq!(initial.files.len(), 3);
    let tab_id = initial.identity.tab_id;
    let snapshot = terminal(&server, &repository).await?;
    for file in snapshot.files() {
        let reference = file
            .review
            .as_ref()
            .ok_or("missing terminal review reference")?
            .reference
            .clone();
        let request = proto::diff_review::encode_set(&SetDiffFileReviewed {
            file: reference,
            reviewed: true,
        });
        client.set_diff_file_reviewed(request.clone()).await?;
        client.set_diff_file_reviewed(request).await?;
    }
    assert!(
        active(&mut client)
            .await?
            .files
            .iter()
            .all(|file| file.review.as_ref().is_some_and(|review| review.reviewed))
    );
    repository.commit_all("reviewed changes");
    repository.git(&[
        "commit",
        "--amend",
        "--allow-empty",
        "-qm",
        "amended message",
    ]);
    client
        .refresh_viewer_tab(v1::RefreshViewerTabRequest {
            tab_id: tab_id.into(),
        })
        .await?;
    assert!(
        active(&mut client)
            .await?
            .files
            .iter()
            .all(|file| file.review.as_ref().is_some_and(|review| review.reviewed))
    );
    repository.write("a.rs", "edited a\n");
    repository.write("binary.bin", b"\0changed!");
    client
        .refresh_viewer_tab(v1::RefreshViewerTabRequest {
            tab_id: tab_id.into(),
        })
        .await?;
    let refreshed = active(&mut client).await?;
    for file in &refreshed.files {
        assert_eq!(
            file.review
                .as_ref()
                .ok_or("missing viewer review")?
                .reviewed,
            file.path.to_str() == Some("b.rs")
        );
    }
    let review = refreshed
        .files
        .iter()
        .find(|file| file.path.to_str() == Some("a.rs"))
        .and_then(|file| file.review.as_ref())
        .ok_or("missing a.rs review")?;
    client
        .set_diff_file_reviewed(proto::diff_review::encode_set(&SetDiffFileReviewed {
            file: review.reference.clone(),
            reviewed: true,
        }))
        .await?;
    let snapshot = terminal(&server, &repository).await?;
    assert!(
        snapshot
            .files()
            .iter()
            .find(|file| file.path == "a.rs")
            .and_then(|file| file.review.as_ref())
            .is_some_and(|review| review.reviewed)
    );
    assert_eq!(
        client
            .set_diff_file_reviewed(v1::SetDiffFileReviewedRequest {
                file: None,
                reviewed: Some(true)
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    server.stop().await?;
    let restarted = ServerHarness::start(&data, Some(data.join("settings.toml"))).await?;
    let mut client = Client::new(restarted.native_channel());
    let restored = active(&mut client).await?;
    assert_eq!(
        restored
            .files
            .iter()
            .map(|file| &file.review)
            .collect::<Vec<_>>(),
        snapshot
            .files()
            .iter()
            .map(|file| &file.review)
            .collect::<Vec<_>>()
    );
    restarted.stop().await?;
    Ok(())
}
