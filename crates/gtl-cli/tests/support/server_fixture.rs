use std::{
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;
use assert_cmd::Command;
use gtl_local_transport::LocalEndpoint;
use gtl_wire::doctor::{Check, DoctorReport};

pub(super) struct ServerFixture {
    temporary: tempfile::TempDir,
    runtime: tokio::runtime::Runtime,
    server: Option<gtl_server::ServerHarness>,
    path: OsString,
}

impl ServerFixture {
    pub(super) fn new() -> Result<Self> {
        let temporary = tempfile::tempdir()?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let data_root = temporary.path().join("data");
        let server = runtime.block_on(gtl_server::ServerHarness::start(&data_root, None))?;
        let cli_binary = std::env::var_os("GTL_CLI_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_git-tools").into());
        fs::copy(cli_binary, temporary.path().join("git-tools"))?;
        executable(
            &temporary.path().join("gtl-server"),
            include_str!("fixtures/server-doctor.sh"),
        )?;
        executable(
            &temporary.path().join("systemctl"),
            include_str!("fixtures/systemctl.sh"),
        )?;
        let path = std::env::join_paths(std::iter::once(temporary.path().to_path_buf()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))?;
        let fixture = Self {
            temporary,
            runtime,
            server: Some(server),
            path,
        };
        let endpoint = LocalEndpoint::from_root(fixture.data_root())?;
        fixture.set_report(&DoctorReport {
            version: env!("CARGO_PKG_VERSION").into(),
            checks: vec![
                Check::pass("endpoint", endpoint.path().display().to_string()),
                Check::pass("settings", "valid"),
            ],
        })?;
        Ok(fixture)
    }

    pub(super) fn installed() -> Result<Self> {
        let fixture = Self::new()?;
        fixture
            .command(&["server", "install"])
            .assert()
            .success()
            .stderr("");
        Ok(fixture)
    }

    pub(super) fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(self.temporary.path().join("git-tools"));
        command
            .args(args)
            .timeout(Duration::from_secs(25))
            .env("PATH", &self.path)
            .env("XDG_CONFIG_HOME", self.config_home())
            .env("GIT_TOOLS_DATA_DIR", self.data_root())
            .env_remove("GIT_TOOLS_CONFIG")
            .env("GTL_TEST_STATE", self.temporary.path().join("state"))
            .env("GTL_TEST_SERVER", self.server_path())
            .env("GTL_TEST_REPORT", self.temporary.path().join("report.json"));
        command
    }

    pub(super) fn set_report(&self, report: &DoctorReport) -> Result<()> {
        fs::write(
            self.temporary.path().join("report.json"),
            serde_json::to_vec(report)?,
        )?;
        Ok(())
    }

    pub(super) fn doctor_report(&self, exit_code: i32) -> Result<DoctorReport> {
        let output = self
            .command(&["doctor", "--json"])
            .assert()
            .code(exit_code)
            .stderr("")
            .get_output()
            .stdout
            .clone();
        Ok(serde_json::from_slice(&output)?)
    }

    pub(super) fn data_root(&self) -> PathBuf {
        self.temporary.path().join("data")
    }
    pub(super) fn config_home(&self) -> PathBuf {
        self.temporary.path().join("config")
    }
    pub(super) fn unit_path(&self) -> PathBuf {
        self.config_home().join("systemd/user/gtl-server.service")
    }
    pub(super) fn server_path(&self) -> PathBuf {
        self.temporary.path().join("gtl-server")
    }
    pub(super) fn service_state(&self) -> Result<Option<String>> {
        let path = self.temporary.path().join("state");
        Ok(if path.try_exists()? {
            Some(fs::read_to_string(path)?.trim().into())
        } else {
            None
        })
    }
}

impl Drop for ServerFixture {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            let _ = self.runtime.block_on(server.stop());
        }
    }
}

fn executable(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}
