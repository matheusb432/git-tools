#![cfg(test)]

//! Integration coverage for `sample_project_manifest_path` with Unix stub scripts.

use std::path::PathBuf;

use gtl_cli::commands::managed::sample_project_manifest_path;

fn unique_temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("git-tools-{name}-{unique}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn sample_project_manifest_path_returns_none_when_binary_is_missing() {
    assert_eq!(
        sample_project_manifest_path("/definitely/not/a/real/binary/xyz-sample_project"),
        None
    );
}

#[cfg(unix)]
#[test]
fn sample_project_manifest_path_returns_stub_stdout_when_target_exists() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_temp_dir("sample_project-manifest-path-stub-ok");
    let target = root.join("resolved/projects.toml");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "").unwrap();
    let stub = root.join("fake-sample_project.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\ncase \"$*\" in\n  'project manifest-path') echo '{}' ;;\n  *) exit 64 ;;\nesac\n",
            target.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(sample_project_manifest_path(stub.to_str().unwrap()), Some(target));
}

#[cfg(unix)]
#[test]
fn sample_project_manifest_path_returns_none_when_stub_path_does_not_exist() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_temp_dir("sample_project-manifest-path-stub-missing-target");
    let stub = root.join("fake-sample_project.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\necho '/definitely/not/a/real/path/projects.toml'\n",
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(sample_project_manifest_path(stub.to_str().unwrap()), None);
}
