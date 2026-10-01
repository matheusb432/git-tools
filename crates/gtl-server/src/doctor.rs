use gtl_infra::{
    app_state::diagnostics::{DatabaseInspection, inspect_database},
    user_config::TomlSettingsStore,
};
use gtl_local_transport::{LocalEndpoint, service::output};
use gtl_wire::doctor::{Check, DoctorReport};

/// Inspects startup prerequisites without creating state or applying migrations.
async fn inspect() -> DoctorReport {
    let mut checks = Vec::new();
    let settings = TomlSettingsStore::from_environment();
    checks.push(match settings.load_viewer_settings() {
        Ok(_) => Check::pass(
            "settings",
            format!(
                "User settings are valid ({})",
                settings
                    .path()
                    .map_or_else(|| "defaults".into(), |path| path.display().to_string())
            ),
        ),
        Err(error) => Check::fail(
            "settings",
            format!("{error:#}"),
            "Correct the setting named in the detail.",
        ),
    });
    checks.push(
        match output(tokio::process::Command::new("git").arg("--version")).await {
            Ok(response) if response.status.success() => {
                Check::pass("Git", String::from_utf8_lossy(&response.stdout).trim())
            }
            Ok(response) => Check::fail(
                "Git",
                String::from_utf8_lossy(&response.stderr).trim(),
                "Install Git and make it available on the registered server's PATH.",
            ),
            Err(error) => Check::fail(
                "Git",
                format!("{error:#}"),
                "Install Git and make it available on the registered server's PATH.",
            ),
        },
    );
    checks.push(match crate::viewer_process::resolve_viewer_bin() {
        Some(path) => Check::pass("viewer executable", path.display().to_string()),
        None => Check::warning(
            "viewer executable",
            "gtl-viewer is unavailable; desktop diff viewing needs it",
            "Install the desktop viewer with `just desktop update`, or use `gtl diff --raw`.",
        ),
    });
    match LocalEndpoint::from_environment() {
        Ok(endpoint) => {
            checks.push(Check::pass(
                "endpoint",
                endpoint.path().display().to_string(),
            ));
            checks.push(Check::pass(
                "database",
                endpoint.data_root().join("gtl.db").display().to_string(),
            ));
            checks.push(match inspect_database(endpoint.data_root()) {
                Ok(DatabaseInspection::Missing) => Check::warning("migrations", "Database has not been created", "Run `gtl server install` to initialize it."),
                Ok(DatabaseInspection::Current) => Check::pass("migrations", "Database integrity and schema version are valid"),
                Ok(DatabaseInspection::Pending { count }) => Check::warning("migrations", format!("{count} migration(s) pending"), "Start the server to apply pending migrations."),
                Err(error) => Check::fail("database health", format!("{error:#}"), "Check the database permissions and use a server release compatible with its schema. Retain the database during recovery."),
            });
        }
        Err(error) => checks.push(Check::fail(
            "endpoint",
            error.to_string(),
            "Correct GIT_TOOLS_DATA_DIR; it must be absolute and fit the platform endpoint.",
        )),
    }
    DoctorReport {
        version: env!("CARGO_PKG_VERSION").into(),
        checks,
    }
}

#[must_use]
pub fn run() -> std::process::ExitCode {
    let report = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(inspect()),
        Err(error) => {
            eprintln!("gtl-server doctor: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("gtl-server doctor: {error}");
            return std::process::ExitCode::FAILURE;
        }
    }
    if report.failed() {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
