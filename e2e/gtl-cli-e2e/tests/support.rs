use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

pub(super) fn workspace_bin(name: &str) -> PathBuf {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "gtl-cli"])
            .status()
            .expect("build gtl-cli");
        assert!(status.success(), "building gtl-cli failed");
    });

    target_dir()
        .join("debug")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn target_dir() -> PathBuf {
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root().join("target"), PathBuf::from);
    if target_dir.is_absolute() {
        target_dir
    } else {
        workspace_root().join(target_dir)
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
}
