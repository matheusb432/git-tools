//! Guards ADR-0003: every OS `#[cfg]` must live inside the gtl-platform crate.
use std::fs;
use std::path::{Path, PathBuf};

const OS_CFG_NEEDLES: &[&str] = &[
    "cfg(target_os", "cfg(windows", "cfg(unix", "cfg(target_family",
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/gtl-platform → up two levels to the root.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
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

#[test]
fn no_os_cfg_outside_pal() {
    let root = workspace_root();
    let pal = root.join("crates/gtl-platform");
    let mut files = Vec::new();
    // Root src/ + every crate's src/; PAL files are excluded below by starts_with(&pal).
    collect_rs(&root.join("src"), &mut files);
    let crate_entries = fs::read_dir(root.join("crates"))
        .expect("crates/ must be readable for the PAL cfg guard");
    for entry in crate_entries.flatten() {
        let member_src = entry.path().join("src");
        if member_src.is_dir() {
            collect_rs(&member_src, &mut files);
        }
    }

    let mut offenders = Vec::new();
    for file in files {
        if file.starts_with(&pal) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&file) else { continue };
        if OS_CFG_NEEDLES.iter().any(|needle| text.contains(needle)) {
            offenders.push(file);
        }
    }
    assert!(
        offenders.is_empty(),
        "OS cfg found outside gtl-platform (ADR-0003 violation): {offenders:?}"
    );
}
