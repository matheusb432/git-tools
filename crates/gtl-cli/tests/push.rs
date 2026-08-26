use std::{
    env,
    path::{Path, PathBuf},
    process,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail, ensure};
use sample_project_client::{ProjectClient as _, project::grpc::SampleGrpcClient};
use sample_project_local_auth::{LocalAuth, ServerEndpoint, ServerInstanceId};
use sample_project_wire::v1::{self, project_service_server::ProjectServiceServer};
use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt as _, str::contains};
use tempfile::TempDir;
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, transport::Server};

mod common;

const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);

struct PushFixture {
    temporary: TempDir,
    repository: PathBuf,
}

impl PushFixture {
    fn new() -> Result<Self> {
        let temporary = tempfile::tempdir().context("temporary push fixture")?;
        let repository = temporary.path().join("repo");
        let remote = temporary.path().join("origin.git");
        std::fs::create_dir_all(&repository).context("create repository directory")?;

        let fixture = Self {
            temporary,
            repository,
        };
        fixture.git(&["init", "-q", "-b", "main"])?;
        fixture.git(&["config", "user.name", "E2E Bot"])?;
        fixture.git(&["config", "user.email", "e2e@example.invalid"])?;
        fixture.git(&["config", "commit.gpgsign", "false"])?;
        fixture.git(&["config", "core.autocrlf", "false"])?;
        fixture.commit("base\n", "chore: base")?;

        let output = process::Command::new("git")
            .args(["init", "--bare", "-q"])
            .arg(&remote)
            .output()
            .context("initialize bare remote")?;
        ensure!(
            output.status.success(),
            "bare remote initialization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let remote = remote.to_string_lossy();
        fixture.git(&["remote", "add", "origin", &remote])?;
        fixture.git(&["push", "-q", "-u", "origin", "main"])?;
        fixture.commit("base\nlocal\n", "feat: local work")?;
        Ok(fixture)
    }

    fn git(&self, arguments: &[&str]) -> Result<String> {
        let output = process::Command::new("git")
            .arg("-C")
            .arg(&self.repository)
            .args(arguments)
            .output()
            .context("run Git")?;
        ensure!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)
            .context("Git stdout is UTF-8")?
            .trim()
            .to_string())
    }

    fn commit(&self, contents: &str, message: &str) -> Result<()> {
        std::fs::write(self.repository.join("work.txt"), contents).context("write fixture file")?;
        self.git(&["add", "work.txt"])?;
        let output = process::Command::new("git")
            .arg("-C")
            .arg(&self.repository)
            .args(["commit", "-q", "-m", message])
            .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00Z")
            .output()
            .context("commit fixture change")?;
        ensure!(
            output.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn write_untracked_file(&self, path: &str, contents: &str) -> Result<()> {
        let path = self.repository.join(path);
        let parent = path
            .parent()
            .context("untracked fixture file has a parent")?;
        std::fs::create_dir_all(parent).context("create untracked fixture directory")?;
        std::fs::write(path, contents).context("write untracked fixture file")
    }

    fn run(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_git-tools"));
        command.args(arguments).current_dir(&self.repository);
        command
    }

    fn config_path(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    fn start_project_catalogue(&self) -> Result<SampleServerHarness> {
        let data_root = self.temporary.path().join("sample_project");
        // This integration-test binary has one test, so no other test can observe these values.
        unsafe {
            env::set_var("HOME", self.temporary.path());
            env::set_var("sample_project_DATA_DIR", &data_root);
        }
        SampleServerHarness::start()
    }

    fn commits_unpushed_count(&self) -> Result<usize> {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])?
            .parse()
            .context("parse unpushed commit count")
    }
}

struct SampleServerHarness {
    data_root: PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SampleServerHarness {
    fn start() -> Result<Self> {
        let data_root = PathBuf::from(
            env::var_os("sample_project_DATA_DIR").context("sample_project fixture data root is configured")?,
        );
        let (shutdown, shutdown_receiver) = oneshot::channel();
        let (failure_sender, failure_receiver) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            if let Err(error) = serve_project_catalogue(shutdown_receiver) {
                drop(failure_sender.send(error.to_string()));
            }
        });
        let server = Self {
            data_root,
            shutdown: Some(shutdown),
            thread: Some(thread),
        };
        Self::wait_until_ready(&failure_receiver)?;
        Ok(server)
    }

    fn data_root(&self) -> &Path {
        &self.data_root
    }

    fn wait_until_ready(failure: &mpsc::Receiver<String>) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build sample_project readiness runtime")?;
        let deadline = Instant::now() + SERVER_START_TIMEOUT;
        let mut last_error = None;

        while Instant::now() < deadline {
            match failure.try_recv() {
                Ok(error) => bail!("sample_project project catalogue server failed: {error}"),
                Err(mpsc::TryRecvError::Disconnected) => {
                    bail!("sample_project project catalogue server exited before readiness");
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            match runtime.block_on(SampleGrpcClient::connect_local()) {
                Ok(client) => match runtime.block_on(client.list_projects()) {
                    Ok(projects)
                        if projects
                            .iter()
                            .any(|project| project.title.to_string() == "repo") =>
                    {
                        return Ok(());
                    }
                    Ok(_) => last_error = Some("fixture project was not returned".to_owned()),
                    Err(error) => last_error = Some(error.to_string()),
                },
                Err(error) => last_error = Some(error.to_string()),
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        bail!(
            "sample_project project catalogue server did not become ready: {}",
            last_error.as_deref().unwrap_or("no response")
        )
    }
}

impl Drop for SampleServerHarness {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

fn serve_project_catalogue(shutdown: oneshot::Receiver<()>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build sample_project project catalogue runtime")?;
    runtime.block_on(async {
        let local_auth = LocalAuth::from_environment().context("open sample_project local server state")?;
        let _capabilities = local_auth
            .load_or_create_server_capabilities()
            .context("create sample_project read capability")?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind sample_project project catalogue server")?;
        let address = listener
            .local_addr()
            .context("read sample_project project catalogue address")?;
        let endpoint = ServerEndpoint::try_new(address, ServerInstanceId::generate())
            .context("validate sample_project project catalogue endpoint")?;
        let _published_endpoint = local_auth
            .publish_endpoint(endpoint)
            .context("publish sample_project project catalogue endpoint")?;

        let project_service = ProjectServiceServer::new(ProjectCatalogueService);
        let (health_reporter, health_service) = tonic_health::server::health_reporter();
        health_reporter
            .set_serving::<ProjectServiceServer<ProjectCatalogueService>>()
            .await;
        Server::builder()
            .add_service(health_service)
            .add_service(project_service)
            .serve_with_incoming_shutdown(
                tonic::transport::server::TcpIncoming::from(listener),
                async move { drop(shutdown.await) },
            )
            .await
            .context("serve sample_project project catalogue")
    })
}

struct ProjectCatalogueService;

#[tonic::async_trait]
impl v1::project_service_server::ProjectService for ProjectCatalogueService {
    async fn get_project(
        &self,
        _request: Request<v1::GetProjectRequest>,
    ) -> Result<Response<v1::GetProjectResponse>, Status> {
        Err(Status::not_found("unknown fixture project"))
    }

    async fn list_active_projects(
        &self,
        _request: Request<v1::ListActiveProjectsRequest>,
    ) -> Result<Response<v1::ListActiveProjectsResponse>, Status> {
        Ok(Response::new(v1::ListActiveProjectsResponse {
            projects: vec![v1::Project {
                id: "REP".to_owned(),
                title: "repo".to_owned(),
                source: Some(v1::ProjectSource {
                    source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                        path: "~/repo".to_owned(),
                    })),
                }),
                git_remote: None,
                mux_session_name: "rep".to_owned(),
                status: v1::ProjectStatus::Active.into(),
                affiliation: v1::ProjectAffiliation::Personal.into(),
                color: None,
                groups: Vec::new(),
            }],
        }))
    }

    async fn pause_project(
        &self,
        _request: Request<v1::PauseProjectRequest>,
    ) -> Result<Response<v1::PauseProjectResponse>, Status> {
        Err(Status::unimplemented("pause_project"))
    }

    async fn resume_project(
        &self,
        _request: Request<v1::ResumeProjectRequest>,
    ) -> Result<Response<v1::ResumeProjectResponse>, Status> {
        Err(Status::unimplemented("resume_project"))
    }

    async fn manage_projects(
        &self,
        _request: Request<v1::ManageProjectsRequest>,
    ) -> Result<Response<v1::ManageProjectsResponse>, Status> {
        Err(Status::unimplemented("manage_projects"))
    }

    async fn unmanage_projects(
        &self,
        _request: Request<v1::UnmanageProjectsRequest>,
    ) -> Result<Response<v1::UnmanageProjectsResponse>, Status> {
        Err(Status::unimplemented("unmanage_projects"))
    }

    async fn prepare_project_clones(
        &self,
        _request: Request<v1::PrepareProjectClonesRequest>,
    ) -> Result<Response<v1::PrepareProjectClonesResponse>, Status> {
        Err(Status::unimplemented("prepare_project_clones"))
    }

    type ExportProjectStream = ReceiverStream<Result<v1::ExportProjectResponse, Status>>;

    async fn export_project(
        &self,
        _request: Request<v1::ExportProjectRequest>,
    ) -> Result<Response<Self::ExportProjectStream>, Status> {
        Err(Status::unimplemented("export_project"))
    }

    type ExportAllProjectsStream = ReceiverStream<Result<v1::ExportAllProjectsResponse, Status>>;

    async fn export_all_projects(
        &self,
        _request: Request<v1::ExportAllProjectsRequest>,
    ) -> Result<Response<Self::ExportAllProjectsStream>, Status> {
        Err(Status::unimplemented("export_all_projects"))
    }

    async fn add_project_session(
        &self,
        _request: Request<v1::AddProjectSessionRequest>,
    ) -> Result<Response<v1::AddProjectSessionResponse>, Status> {
        Err(Status::unimplemented("add_project_session"))
    }

    async fn prepare_project_session_entry(
        &self,
        _request: Request<v1::PrepareProjectSessionEntryRequest>,
    ) -> Result<Response<v1::PrepareProjectSessionEntryResponse>, Status> {
        Err(Status::unimplemented("prepare_project_session_entry"))
    }

    async fn list_project_sessions(
        &self,
        _request: Request<v1::ListProjectSessionsRequest>,
    ) -> Result<Response<v1::ListProjectSessionsResponse>, Status> {
        Err(Status::unimplemented("list_project_sessions"))
    }

    type KillProjectSessionStream = ReceiverStream<Result<v1::KillProjectSessionResponse, Status>>;

    async fn kill_project_session(
        &self,
        _request: Request<tonic::Streaming<v1::KillProjectSessionRequest>>,
    ) -> Result<Response<Self::KillProjectSessionStream>, Status> {
        Err(Status::unimplemented("kill_project_session"))
    }

    async fn style_project_sessions(
        &self,
        _request: Request<v1::StyleProjectSessionsRequest>,
    ) -> Result<Response<v1::StyleProjectSessionsResponse>, Status> {
        Err(Status::unimplemented("style_project_sessions"))
    }
}

fn managed_report(fixture: &PushFixture, config: &Path) -> Result<serde_json::Value> {
    let output = fixture
        .run(&["push", "--all", "--dry", "--json"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).context("parse managed push report")
}

#[test]
fn push_modes_apply_exclusions_and_commit_nested_untracked_files() -> Result<()> {
    let fixture = PushFixture::new()?;
    let project_catalogue = fixture.start_project_catalogue()?;
    let config = fixture.config_path("config.toml");
    let _server = common::ServerHarness::start(Some(&config), Some(project_catalogue.data_root()))?;

    fixture
        .run(&["push"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .code(2)
        .stdout(contains("review before pushing"))
        .stderr(contains("pass --yes"));
    assert_eq!(fixture.commits_unpushed_count()?, 1);

    let report = managed_report(&fixture, &config)?;
    assert_eq!(report["Selected"][0]["Name"], "repo");
    assert_eq!(report["Selected"][0]["Status"], "would-push");
    assert_eq!(report["Excluded"], serde_json::json!([]));

    std::fs::write(
        &config,
        r#"
[[projects]]
name = "unknown"
excluded_from_push_all = true
"#,
    )
    .context("write unknown exclusion config")?;
    let report = managed_report(&fixture, &config)?;
    assert_eq!(report["Selected"][0]["Name"], "repo");
    assert_eq!(report["Excluded"], serde_json::json!([]));

    std::fs::write(
        &config,
        r#"
[push]
confirm = false

[[projects]]
name = "repo"
excluded_from_push_all = true
"#,
    )
    .context("write fixture config")?;
    let report = managed_report(&fixture, &config)?;
    assert_eq!(report["Selected"], serde_json::json!([]));
    assert_eq!(report["Excluded"], serde_json::json!(["repo"]));
    assert_eq!(fixture.commits_unpushed_count()?, 1);

    fixture
        .run(&["p"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .success()
        .stdout(contains("review before pushing").not())
        .stdout(contains("push: pushed 1 commit(s)"))
        .stderr(contains("pass --yes").not());
    assert_eq!(fixture.commits_unpushed_count()?, 0);

    fixture.write_untracked_file("new-work/nested.txt", "new work\n")?;
    fixture
        .run(&["p", "--yes", "feat: add nested work"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .success()
        .stdout(contains("push: staged, committed, and pushed"));
    assert_eq!(fixture.git(&["status", "--porcelain"])?, "");
    assert_eq!(
        fixture.git(&["log", "-1", "--format=%s"])?,
        "feat: add nested work"
    );
    assert_eq!(
        fixture.git(&["show", "origin/main:new-work/nested.txt"])?,
        "new work"
    );
    assert_eq!(fixture.commits_unpushed_count()?, 0);

    let recursive = PushFixture::new()?;
    recursive
        .run(&["push", "--recursive", "--yes"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .stdout(contains("1 repos: 1 pushed"));
    assert_eq!(recursive.commits_unpushed_count()?, 0);
    Ok(())
}
