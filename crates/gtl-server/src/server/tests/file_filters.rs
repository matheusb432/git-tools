use std::time::Duration;

use gtl_application::live_views::save_live_view::{self, SaveLiveView};
use gtl_infra::{app_state::SqliteAppState, clock::SystemClock, git_client::HybridGitClient};
use gtl_models::{diffs::ExcludedExtensions, live_views::LiveComparison, viewer::ViewerTabId};
use gtl_wire::{
    proto, v1,
    viewer::{
        FieldUpdate, ViewerActiveState, ViewerActiveView,
        file_filters::{SetViewerFileFilters, UpdateDiffExclusions, ViewerFileFilters},
    },
};
use tonic::transport::Channel;

use super::{
    ServerHarness, TestResult,
    live_views::{git, ready_shell},
};

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

async fn set(
    client: &mut Client,
    tab_id: ViewerTabId,
    exclusions: FieldUpdate<ExcludedExtensions>,
) -> TestResult {
    let current = filters(client, tab_id).await?;
    client
        .set_viewer_file_filters(proto::viewer::file_filters::encode_set(
            SetViewerFileFilters {
                tab_id,
                exclusions,
                expected: current.saved,
            },
        ))
        .await?;
    Ok(())
}

#[tokio::test]
async fn file_filters_without_project_remain_temporary_and_restore_global_defaults() -> TestResult {
    let Fixture {
        directory,
        repository: _repository,
        database: _database,
        server,
        mut client,
    } = fixture().await?;
    let initial = active(&mut client).await?;
    let tab_id = initial.identity.tab_id;
    assert_eq!(initial.files.len(), 1);
    let choices = filters(&mut client, tab_id).await?;
    assert!(choices.project.is_none());
    assert_eq!(choices.extensions, vec!["lock", "txt"]);
    set(
        &mut client,
        tab_id,
        FieldUpdate::Update(ExcludedExtensions::new(["txt", "lock"])),
    )
    .await?;
    let hidden = active(&mut client).await?;
    assert!(hidden.files.is_empty());
    assert_eq!(
        hidden
            .exclusions
            .as_ref()
            .ok_or("missing exclusions")?
            .hidden_paths
            .len(),
        2
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("settings.toml"))?,
        "[diff]\nexclude = ['lock']\n"
    );
    client
        .update_diff_exclusions(proto::viewer::file_filters::encode_defaults(
            UpdateDiffExclusions {
                project: None,
                extensions: FieldUpdate::Update(ExcludedExtensions::default()),
                expected: Some(ExcludedExtensions::new(["lock"])),
            },
        ))
        .await?;
    let stable = active(&mut client).await?;
    assert_eq!(stable.identity, hidden.identity);
    assert!(stable.files.is_empty());
    set(&mut client, tab_id, FieldUpdate::Clear).await?;
    assert_eq!(active(&mut client).await?.files.len(), 2);
    assert!(filters(&mut client, tab_id).await?.saved.is_none());
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn file_filters_auto_save_empty_project_override_and_restore_removes_it() -> TestResult {
    let Fixture {
        directory: _directory,
        repository,
        database,
        server,
        mut client,
    } = fixture().await?;
    associate(&database, repository.path())?;
    let tab_id = active(&mut client).await?.identity.tab_id;
    let initial = filters(&mut client, tab_id).await?;
    assert!(initial.project.is_some());
    assert!(initial.saved.is_none());
    set(
        &mut client,
        tab_id,
        FieldUpdate::Update(ExcludedExtensions::default()),
    )
    .await?;
    let saved = filters(&mut client, tab_id).await?;
    assert_eq!(saved.saved, Some(ExcludedExtensions::default()));
    assert_eq!(saved.defaults, ExcludedExtensions::new(["lock"]));
    assert_eq!(active(&mut client).await?.files.len(), 2);
    client
        .update_diff_exclusions(proto::viewer::file_filters::encode_defaults(
            UpdateDiffExclusions {
                project: None,
                extensions: FieldUpdate::Update(ExcludedExtensions::new(["txt"])),
                expected: Some(saved.defaults),
            },
        ))
        .await?;
    assert_eq!(
        active(&mut client).await?.files.len(),
        2,
        "saved defaults do not change open tabs"
    );
    set(&mut client, tab_id, FieldUpdate::Clear).await?;
    let restored = filters(&mut client, tab_id).await?;
    assert_eq!(restored.excluded, ExcludedExtensions::new(["txt"]));
    assert!(
        restored.saved.is_none(),
        "restore must remove the project override"
    );
    let settings = proto::viewer::decode_get_viewer_settings_response(
        client
            .get_viewer_settings(v1::GetViewerSettingsRequest {})
            .await?
            .into_inner(),
    )?;
    assert!(
        settings
            .diff_exclusions
            .projects
            .iter()
            .all(|project| !project.configured)
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn file_filters_reject_stale_project_save_without_overwriting_defaults() -> TestResult {
    let Fixture {
        directory: _directory,
        repository,
        database,
        server,
        mut client,
    } = fixture().await?;
    associate(&database, repository.path())?;
    let tab_id = active(&mut client).await?.identity.tab_id;
    set(
        &mut client,
        tab_id,
        FieldUpdate::Update(ExcludedExtensions::new(["txt"])),
    )
    .await?;
    let error = client
        .set_viewer_file_filters(proto::viewer::file_filters::encode_set(
            SetViewerFileFilters {
                tab_id,
                exclusions: FieldUpdate::Clear,
                expected: None,
            },
        ))
        .await
        .err()
        .ok_or("stale write should fail")?;
    assert_eq!(error.code(), tonic::Code::Aborted);
    assert_eq!(
        filters(&mut client, tab_id).await?.saved,
        Some(ExcludedExtensions::new(["txt"]))
    );
    set(&mut client, tab_id, FieldUpdate::Clear).await?;
    assert!(filters(&mut client, tab_id).await?.saved.is_none());
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
    } = fixture().await?;
    server.stop().await?;
    let other = directory.path().join("other");
    git(
        directory.path(),
        &[
            "clone",
            "-q",
            repository.path().to_str().ok_or("repository path")?,
            other.to_str().ok_or("other path")?,
        ],
    )?;
    std::fs::write(other.join("work.txt"), "other change\n")?;
    std::fs::write(other.join("Cargo.lock"), "other lock\n")?;
    save_live_view::execute(
        SaveLiveView {
            path: other,
            comparison: LiveComparison::LocalChanges,
        },
        &HybridGitClient,
        &mut *database.connection_lock()?,
        &SystemClock,
    )?;
    let server = ServerHarness::start(
        directory.path(),
        Some(directory.path().join("settings.toml")),
    )
    .await?;
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
    set(
        &mut client,
        first,
        FieldUpdate::Update(ExcludedExtensions::default()),
    )
    .await?;
    let active_after = active(&mut client).await?;
    assert_eq!(active_before.identity, active_after.identity);
    assert_eq!(active_after.files.len(), 1);
    assert!(filters(&mut client, first).await?.excluded.is_empty());
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
    repository: tempfile::TempDir,
    database: SqliteAppState,
    server: ServerHarness,
    client: Client,
}

async fn fixture() -> TestResult<Fixture> {
    let repository = tempfile::Builder::new()
        .prefix(".gtl-exclusions-")
        .tempdir()?;
    let root = repository.path();
    git(root, &["init", "-q", "-b", "main"])?;
    git(root, &["config", "user.name", "Filters Test"])?;
    git(root, &["config", "user.email", "filters@example.invalid"])?;
    for name in ["work.txt", "Cargo.lock"] {
        std::fs::write(root.join(name), "base\n")?;
    }
    git(root, &["add", "."])?;
    git(root, &["commit", "-qm", "base"])?;
    for name in ["work.txt", "Cargo.lock"] {
        std::fs::write(root.join(name), "changed\n")?;
    }
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    save_live_view::execute(
        SaveLiveView {
            path: root.to_path_buf(),
            comparison: LiveComparison::LocalChanges,
        },
        &HybridGitClient,
        &mut *database.connection_lock()?,
        &SystemClock,
    )?;
    let settings = directory.path().join("settings.toml");
    std::fs::write(&settings, "[diff]\nexclude = ['lock']\n")?;
    let server = ServerHarness::start(directory.path(), Some(settings.clone())).await?;
    let client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    Ok(Fixture {
        directory,
        repository,
        database,
        server,
        client,
    })
}

fn associate(database: &SqliteAppState, root: &std::path::Path) -> TestResult {
    let mut connection = database.connection_lock()?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)",
        [root.to_string_lossy().into_owned()],
    )?;
    transaction.execute("INSERT INTO projects (id, source_id, title, export_include_in_all) VALUES ('FLT', ?1, 'Filters', 1)", [transaction.last_insert_rowid()])?;
    transaction.commit()?;
    Ok(())
}
