//! Guards ADR-0003: every OS `#[cfg]` must live inside the gtl-platform crate.
use std::{
    fs,
    path::{Path, PathBuf},
};

const OS_CFG_NEEDLES: &[&str] = &[
    "cfg(target_os",
    "cfg(windows",
    "cfg(unix",
    "cfg(target_family",
];

fn workspace_root() -> PathBuf {
    // Runtime cwd (cargo runs test binaries from the crate root, .../shared/gtl-platform)
    // rather than compile-time CARGO_MANIFEST_DIR: the shared target dir can reuse a
    // binary built in another checkout/worktree whose baked path no longer exists.
    std::env::current_dir()
        .expect("test binary runs from the crate root")
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git") {
                continue;
            }
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The workspace member directories scanned for OS `#[cfg]` violations — mirrors
/// `check_structure`'s `MEMBER_DIRS`.
const MEMBER_DIRS: [&str; 2] = ["crates", "shared"];

#[test]
fn no_os_cfg_outside_pal() {
    let root = workspace_root();
    let pal = root.join("shared/gtl-platform");
    let mut files = Vec::new();
    // Every crate's src/ under crates/ and shared/; PAL files are excluded below by
    // starts_with(&pal).
    for member_dir in MEMBER_DIRS {
        let member_path = root.join(member_dir);
        let entries = fs::read_dir(&member_path).unwrap_or_else(|_| {
            panic!(
                "{} must be readable for the PAL cfg guard",
                member_path.display()
            )
        });
        for entry in entries.flatten() {
            let member_src = entry.path().join("src");
            if member_src.is_dir() {
                collect_rs(&member_src, &mut files);
            }
        }
    }

    let mut offenders = Vec::new();
    for file in files {
        if file.starts_with(&pal) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        if OS_CFG_NEEDLES.iter().any(|needle| text.contains(needle)) {
            offenders.push(file);
        }
    }
    assert!(
        offenders.is_empty(),
        "OS cfg found outside gtl-platform (ADR-0003 violation): {offenders:?}"
    );
}
