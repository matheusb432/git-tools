#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use gtl_infra::testing::TestRepository;
use gtl_models::failure::{Failure, PushFailure, PushRefRejection};
use gtl_wire::{
    proto, v1,
    viewer::push::{CreateViewerPush, ViewerPushRequest, ViewerPushStatus},
};
use tonic::transport::Channel;

use super::{ServerHarness, TestResult};

type Client = v1::viewer_service_client::ViewerServiceClient<Channel>;

#[cfg(unix)]
struct PushGate(PathBuf);

#[cfg(unix)]
impl Drop for PushGate {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    repository: TestRepository,
    /// Bare repository that `repository` pushes to as `origin`.
    remote: TestRepository,
    server: ServerHarness,
    client: Client,
    base: String,
    first: String,
    latest: String,
}

/// Writes `content` to `file.txt` and commits it with `content` as the message.
fn commit_file_contents(repository: &TestRepository, content: &str) -> String {
    repository.write("file.txt", content);
    repository.commit_all(content)
}

impl Fixture {
    async fn new() -> TestResult<Self> {
        let directory = tempfile::tempdir()?;
        let repository = TestRepository::init(directory.path().join("repo with 'quotes'"));
        let base = commit_file_contents(&repository, "base");
        let remote = repository.add_bare_origin(directory.path().join("remote.git"));
        repository.git(&["branch", "-m", "feature"]);
        let first = commit_file_contents(&repository, "first");
        let latest = commit_file_contents(&repository, "latest");
        let settings = directory.path().join("settings.toml");
        std::fs::write(&settings, "[push]\nconfirm = false\n")?;
        let database = gtl_infra::app_state::SqliteAppState::open(directory.path())?;
        super::seed_live_tabs(&database, [super::unpushed_recipe(repository.root())])?;
        let server = ServerHarness::start(directory.path(), Some(settings)).await?;
        let client = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
        Ok(Self {
            _directory: directory,
            repository,
            remote,
            server,
            client,
            base,
            first,
            latest,
        })
    }

    async fn prepare(&mut self) -> TestResult<ViewerPushRequest> {
        let response = self
            .client
            .create_viewer_push(proto::viewer::push::encode_create(
                CreateViewerPush::Project {
                    path: self.repository.path().to_path_buf().try_into()?,
                },
            ))
            .await?
            .into_inner();
        Ok(ViewerPushRequest {
            id: proto::viewer::push::decode_id(&response.id)?,
        })
    }

    async fn status(&mut self, request: ViewerPushRequest) -> TestResult<ViewerPushStatus> {
        Ok(proto::viewer::push::decode_status(
            self.client
                .get_viewer_push(v1::GetViewerPushRequest {
                    id: request.id.to_string(),
                })
                .await?
                .into_inner(),
        )?)
    }

    async fn submit(&mut self, request: ViewerPushRequest) -> TestResult {
        self.client
            .start_viewer_push(v1::StartViewerPushRequest {
                id: request.id.to_string(),
            })
            .await?;
        Ok(())
    }

    async fn start(&mut self, request: ViewerPushRequest) -> TestResult<ViewerPushStatus> {
        self.submit(request).await?;
        tokio::time::timeout(Duration::from_secs(10), wait_push(self, request)).await?
    }
}

#[tokio::test]
async fn confirmation_pins_sha_and_upstream_while_new_commits_and_dirty_files_stay_local()
-> TestResult {
    let mut fixture = Fixture::new().await?;
    let request = fixture.prepare().await?;
    let ViewerPushStatus::Review(preview) = fixture.status(request).await? else {
        return Err("expected review".into());
    };
    assert_eq!(preview.commit.as_ref(), fixture.latest);
    assert_eq!(preview.count, 2);
    assert!(
        !preview.no_confirmation,
        "CLI opt-out must not bypass viewer confirmation"
    );
    assert_eq!(preview.project, None);
    assert_eq!(preview.branch.as_ref(), "feature");
    assert_eq!(preview.remote_branch.as_ref(), "main");
    assert_eq!(preview.remote.as_ref(), "origin");
    assert_eq!(
        preview.remote_url.as_ref(),
        fixture.remote.path().to_str().ok_or("remote path")?
    );
    assert!(preview.command.contains("push --porcelain"));
    assert!(!preview.command.contains("--atomic"));
    assert!(
        !preview
            .command_arguments
            .contains(&gtl_wire::viewer::push::ViewerPushCommandArgument::Atomic)
    );
    assert!(preview.command.contains("'\\''"));
    assert!(
        preview
            .command
            .ends_with(&format!("{}:refs/heads/main", fixture.latest))
    );
    let newer = commit_file_contents(&fixture.repository, "newer");
    fixture.repository.write("untracked", "local");
    fixture.repository.write("file.txt", "dirty");
    assert_eq!(fixture.start(request).await?, ViewerPushStatus::Succeeded);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.latest);
    assert_eq!(fixture.repository.git(&["rev-parse", "HEAD"]), newer);
    assert_eq!(
        fixture
            .repository
            .git(&["rev-list", "--count", "@{u}..HEAD"]),
        "1"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.repository.path().join("file.txt"))?,
        "dirty"
    );
    assert!(fixture.repository.path().join("untracked").exists());
    assert_eq!(fixture.start(request).await?, ViewerPushStatus::Succeeded);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.latest);
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn confirmation_uses_catalogue_project_title_when_available() -> TestResult {
    let mut fixture = Fixture::new().await?;
    let mut projects =
        v1::project_service_client::ProjectServiceClient::new(fixture.server.native_channel());
    projects
        .create_project(v1::CreateProjectRequest {
            project_id: "PUSH".into(),
            project: Some(v1::ProjectCreation {
                title: "Named project".into(),
                source: Some(v1::ProjectSource {
                    source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                        path: fixture.repository.path().to_string_lossy().into_owned(),
                    })),
                }),
                git_remote: None,
                color: None,
                groups: Vec::new(),
                include_in_full_export: None,
            }),
        })
        .await?;
    let request = fixture.prepare().await?;
    let ViewerPushStatus::Review(preview) = fixture.status(request).await? else {
        return Err("expected review".into());
    };
    assert_eq!(
        preview.project.as_ref().map(AsRef::as_ref),
        Some("Named project")
    );
    assert!(!preview.no_confirmation);
    fixture
        .client
        .edit_settings(proto::viewer::encode_edit_settings_request(
            &gtl_wire::viewer::EditSettingsRequest {
                viewer_push_no_confirmation_projects: gtl_wire::viewer::FieldUpdate::Update(vec![
                    "Another project".to_owned().try_into()?,
                ]),
                ..Default::default()
            },
        ))
        .await?;
    let request = fixture.prepare().await?;
    let ViewerPushStatus::Review(preview) = fixture.status(request).await? else {
        return Err("expected review".into());
    };
    assert!(
        !preview.no_confirmation,
        "other projects retain confirmation"
    );
    fixture
        .client
        .edit_settings(proto::viewer::encode_edit_settings_request(
            &gtl_wire::viewer::EditSettingsRequest {
                viewer_push_no_confirmation_projects: gtl_wire::viewer::FieldUpdate::Update(vec![
                    "Named project".to_owned().try_into()?,
                ]),
                ..Default::default()
            },
        ))
        .await?;
    let request = fixture.prepare().await?;
    let ViewerPushStatus::Review(preview) = fixture.status(request).await? else {
        return Err("expected review".into());
    };
    assert!(preview.no_confirmation);
    Ok(())
}

#[tokio::test]
async fn global_viewer_confirmation_is_independent_of_cli_for_unregistered_repositories()
-> TestResult {
    use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate};

    let mut fixture = Fixture::new().await?;
    let mut settings_client =
        v1::settings_service_client::SettingsServiceClient::new(fixture.server.native_channel());
    for (cli_confirmation, viewer_confirmation) in
        [(true, true), (true, false), (false, true), (false, false)]
    {
        fixture
            .client
            .edit_settings(proto::viewer::encode_edit_settings_request(
                &EditSettingsRequest {
                    push_confirmation_required: FieldUpdate::Update(cli_confirmation),
                    viewer_push_confirmation_required: FieldUpdate::Update(viewer_confirmation),
                    ..Default::default()
                },
            ))
            .await?;
        let request = fixture.prepare().await?;
        let ViewerPushStatus::Review(preview) = fixture.status(request).await? else {
            return Err("expected review".into());
        };
        assert_eq!(preview.project, None);
        assert_eq!(preview.no_confirmation, !viewer_confirmation);
        let cli_requirement = settings_client
            .get_push_confirmation_requirement(v1::GetPushConfirmationRequirementRequest {})
            .await?
            .into_inner();
        assert_eq!(cli_requirement.push_confirmation_required, cli_confirmation);
    }
    let request = fixture.prepare().await?;
    assert_eq!(fixture.start(request).await?, ViewerPushStatus::Succeeded);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.latest);
    fixture.server.stop().await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn confirmed_pushes_continue_in_server_order_after_the_client_disconnects() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let mut fixture = Fixture::new().await?;
    let hooks = fixture.remote.path().join("hooks");
    let gate = PushGate(hooks.join("push-gate"));
    let entered = hooks.join("push-entered");
    std::fs::write(&gate.0, "")?;
    let hook = hooks.join("pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\nhook_dir=\"$(dirname \"$0\")\"\ntouch \"$hook_dir/push-entered\"\nattempt=0\nwhile [ -e \"$hook_dir/push-gate\" ] && [ \"$attempt\" -lt 300 ]; do\n  sleep 0.1\n  attempt=$((attempt + 1))\ndone\n[ ! -e \"$hook_dir/push-gate\" ]\n",
    )?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))?;

    let first = fixture.prepare().await?;
    fixture.submit(first).await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !entered.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;

    let newer = commit_file_contents(&fixture.repository, "newer");
    let second = fixture.prepare().await?;
    fixture.submit(second).await?;
    assert_eq!(fixture.status(first).await?, ViewerPushStatus::Running);
    assert_eq!(fixture.status(second).await?, ViewerPushStatus::Queued);

    fixture.client = Client::new(fixture.server.native_channel());
    std::fs::remove_file(&gate.0)?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), wait_push(&mut fixture, first)).await??,
        ViewerPushStatus::Succeeded
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), wait_push(&mut fixture, second)).await??,
        ViewerPushStatus::Succeeded
    );
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), newer);
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn rewritten_commit_is_rejected_even_while_its_object_still_exists() -> TestResult {
    let mut fixture = Fixture::new().await?;
    let request = fixture.prepare().await?;
    fixture.repository.git(&["reset", "--soft", &fixture.base]);
    fixture.repository.git(&["commit", "-qm", "squashed"]);
    fixture.repository.git(&["cat-file", "-e", &fixture.latest]);
    let ViewerPushStatus::Failed {
        failure: Failure::Push(PushFailure::CommitRemoved { commit }),
    } = fixture.start(request).await?
    else {
        return Err("expected rewrite rejection".into());
    };
    assert_eq!(commit.as_ref(), fixture.latest);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.base);
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn checkout_and_destination_changes_invalidate_pending_reviews() -> TestResult {
    let mut fixture = Fixture::new().await?;
    let request = fixture.prepare().await?;
    fixture.repository.git(&["checkout", "-qb", "other"]);
    fixture
        .repository
        .git(&["branch", "--set-upstream-to=origin/main"]);
    let ViewerPushStatus::Failed {
        failure: Failure::Push(PushFailure::CheckoutChanged { current }),
    } = fixture.start(request).await?
    else {
        return Err("expected checkout rejection".into());
    };
    assert_eq!(current.as_ref(), "other");
    fixture.repository.git(&["checkout", "-q", "feature"]);
    let request = fixture.prepare().await?;
    fixture.repository.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        fixture
            .repository
            .path()
            .to_str()
            .ok_or("repository path")?,
    ]);
    assert_eq!(
        fixture.start(request).await?,
        ViewerPushStatus::Failed {
            failure: Failure::Push(PushFailure::DestinationChanged),
        }
    );
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.base);
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn a_new_revert_does_not_invalidate_the_reviewed_ancestor() -> TestResult {
    let mut fixture = Fixture::new().await?;
    let request = fixture.prepare().await?;
    fixture.repository.git(&["revert", "--no-edit", "HEAD"]);
    assert_eq!(fixture.start(request).await?, ViewerPushStatus::Succeeded);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.latest);
    assert_eq!(
        fixture
            .repository
            .git(&["rev-list", "--count", "@{u}..HEAD"]),
        "1"
    );
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_remote_pushes_without_atomic_support() -> TestResult {
    let mut fixture = Fixture::new().await?;
    fixture
        .remote
        .git(&["config", "receive.advertiseAtomic", "false"]);
    let request = fixture.prepare().await?;
    assert_eq!(fixture.start(request).await?, ViewerPushStatus::Succeeded);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.latest);
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn remote_rejections_are_reported_per_ref() -> TestResult {
    let mut fixture = Fixture::new().await?;
    let request = fixture.prepare().await?;
    let tree = fixture.remote.git(&["rev-parse", "main^{tree}"]);
    let remote_only = fixture.remote.git(&[
        "-c",
        "user.name=Remote",
        "-c",
        "user.email=remote@example.invalid",
        "commit-tree",
        &tree,
        "-p",
        &fixture.base,
        "-m",
        "remote only",
    ]);
    fixture
        .remote
        .git(&["update-ref", "refs/heads/main", &remote_only]);

    let ViewerPushStatus::Failed {
        failure: Failure::Push(PushFailure::Rejected { refs, .. }),
    } = fixture.start(request).await?
    else {
        return Err("expected a remote rejection".into());
    };
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].destination.as_ref(), "refs/heads/main");
    assert_eq!(refs[0].reason, PushRefRejection::FetchFirst);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), remote_only);
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn missing_upstream_and_multiple_push_urls_cannot_prepare_a_push() -> TestResult {
    let mut fixture = Fixture::new().await?;
    fixture.repository.git(&["branch", "--unset-upstream"]);
    let request = fixture.prepare().await?;
    let ViewerPushStatus::Failed {
        failure: Failure::Push(PushFailure::NoUpstream { branch }),
    } = fixture.status(request).await?
    else {
        return Err("expected upstream refusal".into());
    };
    assert_eq!(branch.as_ref(), "feature");
    fixture
        .repository
        .git(&["branch", "--set-upstream-to=origin/main"]);
    fixture.repository.git(&[
        "config",
        "--add",
        "remote.origin.pushurl",
        fixture.remote.path().to_str().ok_or("remote path")?,
    ]);
    fixture.repository.git(&[
        "config",
        "--add",
        "remote.origin.pushurl",
        fixture
            .repository
            .path()
            .to_str()
            .ok_or("repository path")?,
    ]);
    let request = fixture.prepare().await?;
    let ViewerPushStatus::Failed {
        failure: Failure::Push(PushFailure::MultipleDestinations { remote }),
    } = fixture.status(request).await?
    else {
        return Err("expected multiple destination refusal".into());
    };
    assert_eq!(remote.as_ref(), "origin");
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn selected_commit_pushes_only_its_ancestors_and_refreshes_the_live_diff() -> TestResult {
    use gtl_wire::viewer::{ViewerActiveState, ViewerCommitSelection};
    let mut fixture = Fixture::new().await?;
    let shell = tokio::time::timeout(
        Duration::from_secs(10),
        super::live_views::ready_shell(&mut fixture.client),
    )
    .await??;
    let ViewerActiveState::Ready { view } = shell.active else {
        return Err("expected ready diff".into());
    };
    fixture
        .client
        .select_viewer_commit(v1::SelectViewerCommitRequest {
            tab_id: view.identity.tab_id.into(),
            commit_id: fixture.first.clone(),
        })
        .await?;
    let identity = tokio::time::timeout(
        Duration::from_secs(10),
        wait_view(&mut fixture.client, |view| {
            matches!(view.commit_selection, ViewerCommitSelection::Ready { .. })
        }),
    )
    .await??
    .identity;
    let response = fixture
        .client
        .create_viewer_push(proto::viewer::push::encode_create(CreateViewerPush::View {
            identity,
        }))
        .await?
        .into_inner();
    let request = ViewerPushRequest {
        id: proto::viewer::push::decode_id(&response.id)?,
    };
    let ViewerPushStatus::Review(preview) = fixture.status(request).await? else {
        return Err("expected selected review".into());
    };
    assert_eq!(preview.commit.as_ref(), fixture.first);
    assert_eq!(preview.count, 1);
    assert_eq!(fixture.start(request).await?, ViewerPushStatus::Succeeded);
    assert_eq!(fixture.remote.git(&["rev-parse", "main"]), fixture.first);
    tokio::time::timeout(
        Duration::from_secs(10),
        wait_view(&mut fixture.client, |view| {
            view.commit_count == 1 && matches!(view.commit_selection, ViewerCommitSelection::None)
        }),
    )
    .await??;
    fixture.server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn push_availability_reads_current_git_without_replacing_the_displayed_view() -> TestResult {
    use gtl_wire::viewer::push::{ViewerPushAvailability, ViewerPushState};
    let mut fixture = Fixture::new().await?;
    let view = tokio::time::timeout(
        Duration::from_secs(10),
        wait_view(&mut fixture.client, |_| true),
    )
    .await??;
    let request = proto::viewer::push::encode_availability_request(view.identity);
    let read = |response: v1::GetViewerPushAvailabilityResponse| {
        proto::viewer::push::decode_availability(response)
    };
    assert_eq!(
        read(
            fixture
                .client
                .get_viewer_push_availability(request)
                .await?
                .into_inner()
        )?,
        ViewerPushState {
            availability: ViewerPushAvailability::Available,
            snapshot_has_unpushed_commits: Some(true)
        }
    );

    fixture
        .repository
        .git(&["push", "--atomic", "origin", "HEAD:refs/heads/main"]);
    assert_eq!(
        read(
            fixture
                .client
                .get_viewer_push_availability(request)
                .await?
                .into_inner()
        )?,
        ViewerPushState {
            availability: ViewerPushAvailability::NothingToPush,
            snapshot_has_unpushed_commits: Some(false)
        }
    );

    fixture.repository.git(&["reset", "--soft", &fixture.first]);
    let ViewerPushAvailability::Blocked {
        failure: Failure::Push(PushFailure::CommitRemoved { .. }),
    } = read(
        fixture
            .client
            .get_viewer_push_availability(request)
            .await?
            .into_inner(),
    )?
    .availability
    else {
        return Err("rewritten target was reported as pushable".into());
    };
    let unchanged = tokio::time::timeout(
        Duration::from_secs(10),
        wait_view(&mut fixture.client, |_| true),
    )
    .await??;
    assert_eq!(unchanged, view);
    fixture.server.stop().await?;
    Ok(())
}

async fn wait_push(
    fixture: &mut Fixture,
    request: ViewerPushRequest,
) -> TestResult<ViewerPushStatus> {
    loop {
        let status = fixture.status(request).await?;
        if !matches!(status, ViewerPushStatus::Queued | ViewerPushStatus::Running) {
            return Ok(status);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_view(
    client: &mut Client,
    predicate: impl Fn(&gtl_wire::viewer::ViewerActiveView) -> bool,
) -> TestResult<gtl_wire::viewer::ViewerActiveView> {
    loop {
        let shell = super::live_views::ready_shell(client).await?;
        if let gtl_wire::viewer::ViewerActiveState::Ready { view } = shell.active
            && predicate(&view)
        {
            return Ok(*view);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
