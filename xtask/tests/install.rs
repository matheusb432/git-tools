use std::{fs, path::Path, process::Command};

#[test]
fn viewer_install_uses_the_configured_cargo_target_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let target = temporary.path().join("cargo output");
    let bindir = temporary.path().join("bin");
    let name = format!("gtl-viewer{}", std::env::consts::EXE_SUFFIX);
    let release = target.join("release");
    fs::create_dir_all(&release).unwrap();
    fs::write(release.join(&name), b"viewer from the configured target").unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(root)
        .args(["install", "--target", "viewer"])
        .env("CARGO_TARGET_DIR", &target)
        .env("GIT_TOOLS_BINDIR", &bindir)
        .env("XDG_DATA_HOME", temporary.path().join("data"))
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(
        fs::read(bindir.join(name)).unwrap() == b"viewer from the configured target",
        "installed viewer did not come from Cargo's configured target directory"
    );
}
