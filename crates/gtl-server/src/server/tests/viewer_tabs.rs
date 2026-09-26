use std::time::Duration;

use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_models::recipes::RecipeLabel;
use gtl_wire::{
    v1,
    viewer::{ViewerActiveState, ViewerTab, ViewerTabState},
};

use super::{ServerHarness, TestResult, live_views::ready_shell};

type Client = v1::viewer_service_client::ViewerServiceClient<tonic::transport::Channel>;

fn changed_repository(path: std::path::PathBuf) -> TestRepository {
    let repository = TestRepository::init(path);
    repository.write("work.txt", "base\n");
    repository.commit_all("base");
    repository.write("work.txt", "changed\n");
    repository
}

fn repository_name(tab: &ViewerTab) -> String {
    match &tab.label {
        RecipeLabel::Named { name } => name.to_string(),
        RecipeLabel::Repository { repository }
        | RecipeLabel::Changes { repository, .. }
        | RecipeLabel::Compared { repository, .. } => repository.to_string(),
    }
}

/// Waits until no restored tab is still waiting for its first render.
async fn every_tab_ready(client: &mut Client) -> TestResult {
    while !ready_shell(client)
        .await?
        .tabs
        .iter()
        .all(|tab| tab.state == ViewerTabState::Ready)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

#[tokio::test]
async fn open_tabs_survive_a_restart_with_their_order_pins_and_active_tab() -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = changed_repository(directory.path().join("first"));
    let second = changed_repository(directory.path().join("second"));
    let database = SqliteAppState::open(directory.path())?;
    super::seed_live_tabs(
        &database,
        [
            super::working_tree_recipe(first.root()),
            super::working_tree_recipe(second.root()),
        ],
    )?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    let (first_tab, second_tab) = (shell.tabs[0].id, shell.tabs[1].id);
    client
        .set_viewer_tab_pinned(v1::SetViewerTabPinnedRequest {
            tab_id: second_tab.into(),
            pinned: true,
        })
        .await?;
    client
        .activate_viewer_tab(v1::ActivateViewerTabRequest {
            tab_id: first_tab.into(),
        })
        .await?;
    server.stop().await?;

    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let shell = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;

    assert_eq!(
        shell.tabs.iter().map(repository_name).collect::<Vec<_>>(),
        ["second", "first"]
    );
    assert_eq!(
        shell.tabs.iter().map(|tab| tab.pinned).collect::<Vec<_>>(),
        [true, false]
    );
    assert!(shell.tabs.iter().all(|tab| tab.live));
    assert_eq!(
        shell.tabs[0].label,
        RecipeLabel::Compared {
            repository: gtl_models::paths::ProjectName::try_new("second")?,
            base: gtl_models::git::GitRevision::head(),
            head: gtl_models::recipes::RecipeLabelHead::WorkingTree,
        },
        "a restored tab shows its saved label before it renders again"
    );
    let ViewerActiveState::Ready { view } = shell.active else {
        return Err("expected the restored active tab to render".into());
    };
    assert_eq!(view.identity.tab_id, shell.tabs[1].id);
    tokio::time::timeout(Duration::from_secs(10), every_tab_ready(&mut client)).await??;
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn restored_and_reopened_tabs_reuse_history_and_its_comparison_branch() -> TestResult {
    use gtl_application::{
        history::list_recent_render_page::{self, ListRecentRenderPage},
        viewer::saved_tabs,
    };
    let directory = tempfile::tempdir()?;
    let repository = changed_repository(directory.path().join("repo"));
    repository.git(&["branch", "other"]);
    repository.git(&["switch", "-qc", "feature"]);
    repository.git(&["branch", "--set-upstream-to", "main"]);
    repository.commit_all("feature changes");
    let database = SqliteAppState::open(directory.path())?;
    super::seed_live_tabs(&database, [super::unpushed_recipe(repository.root())])?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = Client::new(server.native_channel());
    let initial = tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    client
        .set_viewer_tab_live(v1::SetViewerTabLiveRequest {
            tab_id: initial.tabs[0].id.into(),
            live: false,
        })
        .await?;
    super::wait_for_history(&mut client).await?;
    server.stop().await?;
    let original = {
        let connection = database.connection_lock()?;
        let entries =
            list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection)?
                .entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]
                .comparison_name
                .as_ref()
                .map(AsRef::<str>::as_ref),
            Some("main")
        );
        assert_eq!(
            saved_tabs::load(&connection)?[0].history_id,
            Some(entries[0].id)
        );
        connection.execute_batch("CREATE TABLE history_insertions (id INTEGER); CREATE TRIGGER count_history_insertions AFTER INSERT ON recent_renders BEGIN INSERT INTO history_insertions VALUES (new.id); END;")?;
        entries[0].clone()
    };
    repository.git(&["branch", "--set-upstream-to", "other"]);
    for _ in 0..2 {
        let server = ServerHarness::start(directory.path(), None).await?;
        let mut client = Client::new(server.native_channel());
        let restored =
            tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
        assert_eq!(restored.tabs[0].label, initial.tabs[0].label);
        client
            .close_viewer_tab(v1::CloseViewerTabRequest {
                tab_id: restored.tabs[0].id.into(),
            })
            .await?;
        client
            .open_viewer_history(v1::OpenViewerHistoryRequest {
                render_id: u64::try_from(i64::from(original.id))?,
            })
            .await?;
        let reopened =
            tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
        assert_eq!(reopened.tabs[0].label, initial.tabs[0].label);
        server.stop().await?;
        let connection = database.connection_lock()?;
        let entries =
            list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection)?
                .entries;
        assert_eq!(entries.as_slice(), std::slice::from_ref(&original));
        assert_eq!(
            saved_tabs::load(&connection)?[0].history_id,
            Some(original.id)
        );
        let insertions: i64 =
            connection.query_row("SELECT count(*) FROM history_insertions", [], |row| {
                row.get(0)
            })?;
        assert_eq!(
            insertions, 0,
            "restoring and reopening must not start another history entry"
        );
    }
    // Pruning history severs the saved reference; the next restore records its missing entry.
    database
        .connection_lock()?
        .execute("DELETE FROM recent_renders", [])?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = Client::new(server.native_channel());
    tokio::time::timeout(Duration::from_secs(10), ready_shell(&mut client)).await??;
    super::wait_for_history(&mut client).await?;
    server.stop().await?;
    let connection = database.connection_lock()?;
    let entries =
        list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection)?.entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].comparison_name, original.comparison_name);
    assert_eq!(
        saved_tabs::load(&connection)?[0].history_id,
        Some(entries[0].id)
    );
    Ok(())
}
