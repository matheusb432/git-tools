use std::time::Duration;

use anyhow::Result;
use assert_cmd::Command;
use gtl_wire::v1;
use predicates::{prelude::PredicateBooleanExt as _, str::contains};

mod common;

fn command(arguments: &[&str]) -> Command {
    let executable = std::env::var_os("GTL_CLI_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_git-tools").into());
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(15));
    command
}

fn check_json_input_contracts(payload: &serde_json::Value) -> Result<()> {
    let isolation = tempfile::tempdir()?;
    let missing_endpoint = isolation.path().join("missing-server");
    command(&["project", "--help"])
        .env("GIT_TOOLS_DATA_DIR", &missing_endpoint)
        .assert()
        .success()
        .stdout(contains("add"))
        .stderr("");
    command(&["project", "add", "--help"])
        .env("GIT_TOOLS_DATA_DIR", &missing_endpoint)
        .assert()
        .success()
        .stdout(contains("<JSON_PAYLOAD>").and(contains("include_in_full_export")))
        .stderr("");
    let mut invalid_payloads = vec![
        "{".to_owned(),
        "[]".to_owned(),
        "{}".to_owned(),
        "null".to_owned(),
        r#"{"project_id":"APP","project_id":"TEST"}"#.to_owned(),
    ];
    for (field, value) in [
        ("project_id", serde_json::json!("A1")),
        ("title", serde_json::json!("")),
        ("title", serde_json::json!(" App ")),
        (
            "source",
            serde_json::json!({"kind": "directory", "path": "relative"}),
        ),
        (
            "source",
            serde_json::json!({"kind": "remote", "path": "/srv/app"}),
        ),
        ("git_remote", serde_json::json!("")),
        ("color", serde_json::json!("#A0DD72")),
        ("groups", serde_json::json!(["tools", "tools"])),
        ("groups", serde_json::json!([" "])),
        (
            "groups",
            serde_json::json!((0..65).map(|index| index.to_string()).collect::<Vec<_>>()),
        ),
        ("include_in_full_export", serde_json::json!("false")),
        ("unknown_field", serde_json::json!(true)),
    ] {
        let mut invalid = payload.clone();
        invalid[field] = value;
        invalid_payloads.push(invalid.to_string());
    }
    for payload in invalid_payloads {
        command(&["project", "add", &payload])
            .env("GIT_TOOLS_DATA_DIR", &missing_endpoint)
            .assert()
            .code(2)
            .stdout("")
            .stderr(contains("JSON_PAYLOAD"));
    }
    command(&["project", "add"])
        .env("GIT_TOOLS_DATA_DIR", &missing_endpoint)
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("JSON_PAYLOAD"));
    command(&["project", "add", &payload.to_string()])
        .env("GIT_TOOLS_DATA_DIR", &missing_endpoint)
        .assert()
        .code(4)
        .stdout("")
        .stderr(contains(
            "project add: Could not reach the local gtl-server.",
        ));
    assert!(!missing_endpoint.exists());
    Ok(())
}

#[test]
fn project_add_validates_json_and_creates_projects_through_the_server() -> Result<()> {
    let repository = tempfile::tempdir()?;
    let payload = serde_json::json!({
        "project_id": "app",
        "title": "My app",
        "source": {"kind": "directory", "path": repository.path()},
    });
    check_json_input_contracts(&payload)?;
    let _server = common::ServerHarness::start(None, None)?;

    command(&["project", "add", &payload.to_string()])
        .assert()
        .success()
        .stdout("Added project: APP\n")
        .stderr("");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = runtime.block_on(gtl_client::GtlClient::connect_local())?;
    let project = runtime.block_on(client.get_project(v1::GetProjectRequest {
        project_id: "APP".to_owned(),
    }))?;
    assert_eq!(project.id, "APP");
    assert_eq!(project.title, "My app");
    assert_eq!(project.status(), v1::ProjectStatus::Active);
    assert_eq!(project.git_remote, None);
    assert_eq!(project.color, None);
    assert!(project.groups.is_empty());
    let Some(v1::project_source::Source::Directory(directory)) =
        project.source.and_then(|source| source.source)
    else {
        anyhow::bail!("created project has no directory source");
    };
    assert_eq!(directory.path, repository.path().to_string_lossy());

    command(&["project", "add", &payload.to_string(), "--json"])
        .assert()
        .code(3)
        .stdout("")
        .stderr(contains("project add:"));

    let repository_custom = tempfile::tempdir()?;
    let payload = serde_json::json!({
        "project_id": "FULL",
        "title": "Custom project",
        "source": {"kind": "directory", "path": repository_custom.path()},
        "git_remote": "https://example.invalid/owner/project.git",
        "color": "#a0dd72",
        "groups": ["tools", "personal"],
        "include_in_full_export": false,
    });
    let added = command(&["project", "add", &payload.to_string(), "--json"])
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&added)?,
        serde_json::json!({"project_id": "FULL", "status": "active"})
    );
    let project = runtime.block_on(client.get_project(v1::GetProjectRequest {
        project_id: "FULL".to_owned(),
    }))?;
    assert_eq!(project.title, "Custom project");
    assert_eq!(
        project.git_remote.as_deref(),
        Some("https://example.invalid/owner/project.git")
    );
    assert_eq!(project.color.as_deref(), Some("#a0dd72"));
    assert_eq!(project.groups, ["personal", "tools"]);
    let projects =
        runtime.block_on(client.list_active_projects(v1::ListActiveProjectsRequest {}))?;
    assert_eq!(
        projects
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>(),
        ["APP", "FULL"]
    );
    Ok(())
}
