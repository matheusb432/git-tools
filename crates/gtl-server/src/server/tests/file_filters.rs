use std::time::Duration;

use gtl_application::ports::{ExtensionFilterReader as _, ExtensionFilterWriter as _};
use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_models::{
    diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions},
    viewer::ViewerTabId,
};
use gtl_wire::{
    proto, v1,
    viewer::{
        ViewerActiveState, ViewerActiveView,
        file_filters::{SetViewerFileFilters, ViewerFileFilters},
    },
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

async fn filters(client: &mut Client, tab_id: ViewerTabId) -> TestResult<ViewerFileFilters> {
    Ok(proto::viewer::file_filters::decode_filters(
        client
            .get_viewer_file_filters(proto::viewer::file_filters::encode_get(tab_id))
            .await?
            .into_inner(),
    )?)
}

async fn set(client: &mut Client, tab_id: ViewerTabId, filter: ExtensionFilter) -> TestResult {
    client
        .set_viewer_file_filters(proto::viewer::file_filters::encode_set(
            SetViewerFileFilters { tab_id, filter },
        ))
        .await?;
    Ok(())
}

fn filter(mode: ExtensionFilterMode, extensions: &[&str]) -> ExtensionFilter {
    ExtensionFilter::new(mode, FileExtensions::new(extensions.iter().copied()))
}

#[tokio::test]
async fn file_filters_apply_both_modes_to_the_tab_and_save_them_for_the_repository() -> TestResult {
    let Fixture {
        directory: _directory,
        repository,
        database,
        server,
        mut client,
    } = fixture(None).await?;
    let initial = active(&mut client).await?;
    let tab_id = initial.identity.tab_id;
    assert_eq!(initial.files.len(), 2);
    let choices = filters(&mut client, tab_id).await?;
    assert_eq!(choices.filter, ExtensionFilter::default());
    assert_eq!(choices.extensions, vec!["lock", "txt"]);

    let hide_locks = filter(ExtensionFilterMode::Hide, &["lock"]);
    set(&mut client, tab_id, hide_locks.clone()).await?;
    let hidden = active(&mut client).await?;
    assert_eq!(hidden.files.len(), 1);
    assert_eq!(hidden.files[0].path.to_string_lossy(), "work.txt");
    assert_eq!(filters(&mut client, tab_id).await?.filter, hide_locks);
    assert_eq!(database.extension_filter(&repository.root())?, hide_locks);

    let only_locks = filter(ExtensionFilterMode::Only, &["lock"]);
    set(&mut client, tab_id, only_locks.clone()).await?;
    let focused = active(&mut client).await?;
    assert_eq!(focused.files.len(), 1);
    assert_eq!(focused.files[0].path.to_string_lossy(), "Cargo.lock");
    assert_eq!(
        focused
            .extension_filter
            .as_ref()
            .ok_or("missing applied filter")?
            .filter,
        only_locks
    );
    assert_eq!(database.extension_filter(&repository.root())?, only_locks);

    set(&mut client, tab_id, ExtensionFilter::default()).await?;
    let cleared = active(&mut client).await?;
    assert_eq!(cleared.files.len(), 2);
    assert!(cleared.extension_filter.is_none());
    assert_eq!(
        database.extension_filter(&repository.root())?,
        ExtensionFilter::default()
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn saved_repository_filters_apply_when_a_tab_opens() -> TestResult {
    let Fixture {
        directory: _directory,
        repository: _repository,
        database: _database,
        server,
        mut client,
    } = fixture(Some(filter(ExtensionFilterMode::Only, &["txt"]))).await?;
    let view = active(&mut client).await?;
    assert_eq!(view.files.len(), 1);
    assert_eq!(view.files[0].path.to_string_lossy(), "work.txt");
    assert_eq!(
        filters(&mut client, view.identity.tab_id).await?.filter,
        filter(ExtensionFilterMode::Only, &["txt"])
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn file_filters_can_finish_for_an_inactive_tab_without_changing_the_active_review()
-> TestResult {
    let Fixture {
        directory,
        repository,
        database,
        server,
        client: _client,
    } = fixture(Some(filter(ExtensionFilterMode::Hide, &["lock"]))).await?;
    server.stop().await?;
    let other = directory.path().join("other");
    repository.git(&[
        "clone",
        "-q",
        repository.path().to_str().ok_or("repository path")?,
        other.to_str().ok_or("other path")?,
    ]);
    std::fs::write(other.join("work.txt"), "other change\n")?;
    std::fs::write(other.join("Cargo.lock"), "other lock\n")?;
    super::seed_live_tabs(
        &database,
        [super::working_tree_recipe(
            gtl_models::paths::RepositoryRoot::try_new(other)?,
        )],
    )?;
    let data = directory.path().join("data");
    let server = ServerHarness::start(&data, Some(data.join("settings.toml"))).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    assert_eq!(shell.tabs.len(), 2);
    let first = shell.tabs[0].id;
    let other = shell.tabs[1].id;
    client
        .activate_viewer_tab(v1::ActivateViewerTabRequest {
            tab_id: first.into(),
        })
        .await?;
    assert_eq!(active(&mut client).await?.files.len(), 1);
    client
        .activate_viewer_tab(v1::ActivateViewerTabRequest {
            tab_id: other.into(),
        })
        .await?;
    let active_before = active(&mut client).await?;
    set(&mut client, first, ExtensionFilter::default()).await?;
    let active_after = active(&mut client).await?;
    assert_eq!(active_before.identity, active_after.identity);
    assert_eq!(active_after.files.len(), active_before.files.len());
    assert!(!filters(&mut client, first).await?.filter.is_active());
    client
        .activate_viewer_tab(v1::ActivateViewerTabRequest {
            tab_id: first.into(),
        })
        .await?;
    assert_eq!(active(&mut client).await?.files.len(), 2);
    server.stop().await?;
    Ok(())
}

struct Fixture {
    directory: tempfile::TempDir,
    repository: TestRepository,
    database: SqliteAppState,
    server: ServerHarness,
    client: Client,
}

async fn fixture(saved: Option<ExtensionFilter>) -> TestResult<Fixture> {
    let directory = tempfile::tempdir()?;
    let repository =
        TestRepository::init(std::fs::canonicalize(directory.path())?.join("repository"));
    for name in ["work.txt", "Cargo.lock"] {
        repository.write(name, "base\n");
    }
    repository.commit_all("base");
    for name in ["work.txt", "Cargo.lock"] {
        repository.write(name, "changed\n");
    }
    let data = directory.path().join("data");
    std::fs::create_dir(&data)?;
    let database = SqliteAppState::open(&data)?;
    if let Some(saved) = saved {
        database.save_extension_filter(&repository.root(), &saved)?;
    }
    super::seed_live_tabs(&database, [super::working_tree_recipe(repository.root())])?;
    let server = ServerHarness::start(&data, Some(data.join("settings.toml"))).await?;
    let client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    Ok(Fixture {
        directory,
        repository,
        database,
        server,
        client,
    })
}
