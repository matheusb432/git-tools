use anyhow::Result;
use assert_cmd::Command;
use gtl_wire::v1;
use predicates::str::contains;

mod common;

fn command(arguments: &[&str]) -> Command {
    let executable = std::env::var_os("GTL_CLI_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_git-tools").into());
    let mut command = Command::new(executable);
    command.args(arguments).env("NO_COLOR", "1");
    command
}

fn active_project_ids() -> Result<Vec<String>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = gtl_client::GtlClient::connect_local().await?;
        let response = client
            .list_active_projects(v1::ListActiveProjectsRequest {})
            .await?;
        Ok(response
            .projects
            .into_iter()
            .map(|project| project.id)
            .collect())
    })
}

#[test]
fn project_pause_and_resume_report_idempotence_and_preserve_membership() -> Result<()> {
    let repository = tempfile::tempdir()?;
    let _server = common::ServerHarness::start(None, Some(repository.path()))?;
    assert_eq!(active_project_ids()?, ["RP".to_owned()]);

    let paused = command(&["project", "pause", "rp", "--json"])
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&paused)?,
        serde_json::json!({"project_id": "RP", "status": "paused", "changed": true})
    );
    assert!(active_project_ids()?.is_empty());
    command(&["project", "pause", "RP"])
        .assert()
        .success()
        .stdout("Already paused project: RP\n")
        .stderr("");

    let resumed = command(&["project", "resume", "rp", "--json"])
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&resumed)?,
        serde_json::json!({"project_id": "RP", "status": "active", "changed": true})
    );
    assert_eq!(active_project_ids()?, ["RP".to_owned()]);
    command(&["project", "resume", "RP"])
        .assert()
        .success()
        .stdout("Already active project: RP\n")
        .stderr("");

    command(&["project", "pause", "RP"])
        .assert()
        .success()
        .stdout("Paused project: RP\n")
        .stderr("");
    command(&["project", "resume", "RP"])
        .assert()
        .success()
        .stdout("Resumed project: RP\n")
        .stderr("");

    for verb in ["pause", "resume"] {
        command(&["project", verb, "--help"])
            .assert()
            .success()
            .stderr("")
            .stdout(contains(format!(
                "Usage: git-tools project {verb} [OPTIONS] <PROJECT_ID>"
            )));
    }

    for (arguments, error) in [
        (&["project", "pause"][..], "PROJECT_ID"),
        (&["project", "pause", "toolong"], "project ID must contain"),
    ] {
        command(arguments)
            .assert()
            .code(2)
            .stdout("")
            .stderr(contains(error));
    }
    command(&["project", "pause", "NO"])
        .assert()
        .code(3)
        .stdout("")
        .stderr(contains(
            "project pause: This project is no longer available.",
        ));
    assert_eq!(active_project_ids()?, ["RP".to_owned()]);
    Ok(())
}
