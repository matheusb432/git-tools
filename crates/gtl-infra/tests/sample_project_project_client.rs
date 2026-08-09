#![cfg(unix)]

use std::{os::unix::fs::PermissionsExt as _, path::Path, time::Duration};

use gtl_application::ports::ProjectClientError;
use gtl_infra::sample_project_project_client::SampleProjectClient;

const PROJECTS_JSON: &str = r##"[
    {
        "id": "GTL",
        "title": "git-tools",
        "mux_session_name": "git-tools",
        "source": {"kind": "directory", "value": "~/tools/git-tools"},
        "git_remote": "git@example.test:tools/git-tools.git",
        "is_paused": false,
        "affiliation": "personal",
        "color": "#123abc",
        "groups": ["rust"]
    },
    {
        "id": "sample_project",
        "title": "sample_project",
        "mux_session_name": "sample_project",
        "source": {"kind": "directory", "value": "/srv/sample_project"},
        "git_remote": null,
        "is_paused": false,
        "affiliation": "personal",
        "color": null,
        "groups": []
    }
]"##;
const FIXTURE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
static sample_project_PROCESS_FIXTURE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn invokes_json_project_list_and_uses_sample_project_owned_fields() -> anyhow::Result<()> {
    let _fixture_guard = lock_process_fixture().await?;
    let directory = tempfile::tempdir()?;
    let script = directory.path().join("sample_project");
    write_executable(
        &script,
        &format!(
            "#!/bin/sh\n[ \"$#\" -eq 3 ] && [ \"$1\" = project ] && [ \"$2\" = list ] && [ \"$3\" = --json ] || exit 64\nprintf '%s' '{PROJECTS_JSON}'\n"
        ),
    )?;
    let client = SampleProjectClient::new(script.as_os_str(), "/home/u");

    let projects = client.list_projects().await?;

    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].name, "git-tools");
    assert_eq!(projects[0].path, Path::new("/home/u/tools/git-tools"));
    assert_eq!(projects[0].remote, "git@example.test:tools/git-tools.git");
    assert_eq!(projects[1].name, "sample_project");
    assert_eq!(projects[1].path, Path::new("/srv/sample_project"));
    assert_eq!(projects[1].remote, "");
    Ok(())
}

#[tokio::test]
async fn reports_nonzero_exit_status_and_stderr() -> anyhow::Result<()> {
    let _fixture_guard = lock_process_fixture().await?;
    let directory = tempfile::tempdir()?;
    let script = directory.path().join("sample_project");
    write_executable(
        &script,
        "#!/bin/sh\nprintf '%s\\n' 'project catalogue unavailable' >&2\nexit 7\n",
    )?;
    let client = SampleProjectClient::new(script.as_os_str(), "/home/u");

    let Err(error) = client.list_projects().await else {
        anyhow::bail!("unsuccessful sample_project command succeeded");
    };

    assert!(matches!(error, ProjectClientError::Unavailable { .. }));
    let message = error.to_string();
    assert!(message.contains("exit status: 7"), "{message}");
    assert!(
        message.contains("project catalogue unavailable"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn reports_malformed_json_as_invalid_data() -> anyhow::Result<()> {
    let _fixture_guard = lock_process_fixture().await?;
    let directory = tempfile::tempdir()?;
    let script = directory.path().join("sample_project");
    write_executable(&script, "#!/bin/sh\nprintf '%s' '[{not-json}]'\n")?;
    let client = SampleProjectClient::new(script.as_os_str(), "/home/u");

    let Err(error) = client.list_projects().await else {
        anyhow::bail!("malformed sample_project JSON was accepted");
    };

    assert!(matches!(error, ProjectClientError::InvalidData { .. }));
    assert!(error.to_string().contains("parsing JSON"), "{error}");
    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

async fn lock_process_fixture() -> anyhow::Result<tokio::sync::MutexGuard<'static, ()>> {
    tokio::time::timeout(FIXTURE_LOCK_TIMEOUT, sample_project_PROCESS_FIXTURE.lock())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for the sample_project process fixture"))
}
