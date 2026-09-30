use std::time::Duration;

use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_models::{diffs::CommitTimeRange, timestamps::MachineTimestamp};
use gtl_wire::{
    proto, v1,
    viewer::{
        ViewerActiveState,
        commit_search::{SearchViewerCommits, ViewerCommitSearchScope},
    },
};

use super::{ServerHarness, TestResult, live_views::ready_shell};

#[tokio::test]
async fn search_keeps_the_full_pinned_commit_list_and_isolates_the_active_branch() -> TestResult {
    let (_directory, repository, server, mut client, identity) = search_fixture(102).await?;
    let search = |scope, query: &str| {
        proto::viewer::encode_search_viewer_commits_request(SearchViewerCommits {
            scope,
            query: query.into(),
            time_range: CommitTimeRange::default(),
        })
    };
    let found = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::Snapshot(identity),
            "checkpoint 0",
        ))
        .await?
        .into_inner();
    assert_eq!(found.commits[0].subject, "checkpoint 0");
    let oldest = found.commits[0].id.clone();
    repository.write("work.txt", "newer outside snapshot\n");
    repository.commit_all("newer outside snapshot");
    repository.git(&["switch", "-qc", "other"]);
    repository.write("work.txt", "other branch\n");
    repository.commit_all("other branch only");
    repository.git(&["update-ref", "refs/remotes/origin/other", "HEAD"]);
    repository.git(&["tag", "other-tag"]);
    repository.git(&["switch", "feature"]);
    let pinned = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::Snapshot(identity),
            "outside snapshot",
        ))
        .await?
        .into_inner();
    assert_eq!(pinned.total_matches, 0);
    let branch = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::ActiveBranchSnapshot(identity),
            "outside snapshot",
        ))
        .await?
        .into_inner();
    assert_eq!(branch.total_matches, 1);
    assert_eq!(branch.branch.as_deref(), Some("feature"));
    let unrelated = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::ActiveBranch(repository.root()),
            "other branch only",
        ))
        .await?
        .into_inner();
    assert_eq!(unrelated.total_matches, 0);
    let all = client
        .search_viewer_commits(search(ViewerCommitSearchScope::Snapshot(identity), ""))
        .await?
        .into_inner();
    assert_eq!(all.total_matches, 102);
    assert_eq!(all.commits.len(), 100);
    client
        .select_viewer_commit(v1::SelectViewerCommitRequest {
            tab_id: identity.tab_id.into(),
            commit_id: oldest,
        })
        .await?;
    let selected =
        tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view: selected } = selected.active else {
        return Err("missing selection".into());
    };
    let all = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::Snapshot(selected.identity),
            "",
        ))
        .await?
        .into_inner();
    assert_eq!(
        all.total_matches, 102,
        "selection must retain the snapshot's search scope"
    );
    let error = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::ActiveBranch(repository.root()),
            &"x".repeat(257),
        ))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn opening_branch_matches_preserves_the_snapshot_and_supports_root_commits() -> TestResult {
    let (_directory, repository, server, mut client, _identity) = search_fixture(2).await?;
    let search = |scope, query: &str| {
        proto::viewer::encode_search_viewer_commits_request(SearchViewerCommits {
            scope,
            query: query.into(),
            time_range: CommitTimeRange::default(),
        })
    };
    let oldest = repository.git(&["rev-parse", "HEAD~1"]);
    repository.git(&["switch", "-qc", "other"]);
    repository.write("work.txt", "other branch\n");
    repository.commit_all("other branch only");
    let other_id = repository.git(&["rev-parse", "HEAD"]);
    repository.git(&["switch", "feature"]);
    let opened = client
        .open_viewer_commit(v1::OpenViewerCommitRequest {
            scope: Some(v1::open_viewer_commit_request::Scope::ActiveBranchPath(
                repository.root().to_string(),
            )),
            id: oldest,
        })
        .await?
        .into_inner();
    assert!(opened.tab_id > 0);
    let opened_shell =
        tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view: opened_view } = opened_shell.active else {
        return Err("missing opened commit".into());
    };
    assert_eq!(opened_view.commit_count, 1);
    let opened_commits = client
        .search_viewer_commits(search(
            ViewerCommitSearchScope::Snapshot(opened_view.identity),
            "",
        ))
        .await?
        .into_inner();
    assert_eq!(opened_commits.commits[0].subject, "checkpoint 0");
    assert_eq!(
        opened_shell.tabs.len(),
        2,
        "opening a match preserves the pinned snapshot"
    );
    let base_id = repository.git(&["rev-parse", "main"]);
    client
        .open_viewer_commit(v1::OpenViewerCommitRequest {
            scope: Some(v1::open_viewer_commit_request::Scope::ActiveBranchPath(
                repository.root().to_string(),
            )),
            id: base_id,
        })
        .await?;
    let root_shell =
        tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view: root_view } = root_shell.active else {
        return Err("missing root commit".into());
    };
    assert_eq!(root_view.commit_count, 1);
    assert_eq!(root_view.files.len(), 1);
    let error = client
        .open_viewer_commit(v1::OpenViewerCommitRequest {
            scope: Some(v1::open_viewer_commit_request::Scope::ActiveBranchPath(
                repository.root().to_string(),
            )),
            id: other_id,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Aborted);
    server.stop().await?;
    Ok(())
}

type Client = v1::viewer_service_client::ViewerServiceClient<tonic::transport::Channel>;

#[tokio::test]
async fn time_search_filters_all_scopes_before_the_result_limit() -> TestResult {
    let (_directory, repository, server, mut client, identity) = search_fixture(102).await?;
    for scope in [
        ViewerCommitSearchScope::Snapshot(identity),
        ViewerCommitSearchScope::ActiveBranchSnapshot(identity),
        ViewerCommitSearchScope::ActiveBranch(repository.root()),
    ] {
        let request = SearchViewerCommits {
            scope,
            query: String::new(),
            time_range: CommitTimeRange::new(
                Some("2026-09-29T09:00:00-03:00".try_into()?),
                Some("2026-09-29T12:01:00Z".try_into()?),
            )?,
        };
        let found = client
            .search_viewer_commits(proto::viewer::encode_search_viewer_commits_request(
                request.clone(),
            ))
            .await?
            .into_inner();
        assert_eq!(found.total_matches, 2);
        assert_eq!(
            found
                .commits
                .iter()
                .map(|commit| commit.subject.as_str())
                .collect::<Vec<_>>(),
            ["checkpoint 1", "checkpoint 0"]
        );
        let found = client
            .search_viewer_commits(proto::viewer::encode_search_viewer_commits_request(
                SearchViewerCommits {
                    query: "checkpoint 0".into(),
                    ..request
                },
            ))
            .await?
            .into_inner();
        assert_eq!(found.total_matches, 1);
        assert_eq!(found.commits[0].subject, "checkpoint 0");
    }
    let error = client
        .search_viewer_commits(v1::SearchViewerCommitsRequest {
            scope: Some(v1::search_viewer_commits_request::Scope::ActiveBranchPath(
                repository.root().to_string(),
            )),
            time_from: Some("2026-09-30T12:00:00Z".into()),
            time_until: Some("2026-09-29T12:00:00Z".into()),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    server.stop().await?;
    Ok(())
}

async fn search_fixture(
    count: u32,
) -> TestResult<(
    tempfile::TempDir,
    TestRepository,
    ServerHarness,
    Client,
    gtl_wire::viewer::ViewerViewIdentity,
)> {
    let directory = tempfile::tempdir()?;
    let repository = TestRepository::init(directory.path().join("repo"));
    repository.write("work.txt", "base\n");
    repository.commit_all("base");
    repository.git(&["switch", "-qc", "feature"]);
    repository.git(&["branch", "--set-upstream-to", "main"]);
    for index in 0..count {
        repository.write("work.txt", format!("checkpoint {index}\n"));
        repository.git(&["add", "--all"]);
        let date = MachineTimestamp::from_unix_seconds(
            "2026-09-29T12:00:00Z"
                .parse::<MachineTimestamp>()?
                .instant()
                .as_second()
                + i64::from(index) * 60,
        )?;
        repository.git(&[
            "commit",
            "-q",
            "--date",
            date.as_ref(),
            "-m",
            &format!("checkpoint {index}"),
        ]);
    }
    let database = SqliteAppState::open(directory.path())?;
    super::seed_live_tabs(&database, [super::unpushed_recipe(repository.root())])?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let initial = tokio::time::timeout(Duration::from_secs(15), ready_shell(&mut client)).await??;
    let ViewerActiveState::Ready { view } = initial.active else {
        return Err("missing snapshot".into());
    };
    client
        .set_viewer_tab_live(v1::SetViewerTabLiveRequest {
            tab_id: view.identity.tab_id.into(),
            live: false,
        })
        .await?;
    Ok((directory, repository, server, client, view.identity))
}
