//! Deterministic Dioxus Web release staging for the Tauri viewer.

use std::{
    fmt::Write as FmtWrite,
    fs::{self, File},
    io::{Read, Write as IoWrite},
    path::{Path, PathBuf},
    process::{Child, Command},
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};

use super::frontend;
use crate::{
    process::{self, Status},
    project,
    task::Step,
    verb::Verb,
};

const DIOXUS_CLI_VERSION: &str = "0.7.10";
const DIST_DIRECTORY: &str = "crates/gtl-web/dist";
const PUBLIC_DIRECTORY: &str = "crates/gtl-web/dist/public";
const SOURCE_FINGERPRINT_PATH: &str = "crates/gtl-web/dist/.source-fingerprint";
const BUNDLE_FINGERPRINT_PATH: &str = "crates/gtl-web/dist/.bundle-fingerprint";
const DIOXUS_INTERNAL_RELEASE_DIRECTORY: &str = "dx/gtl-web/release/web";
const SOURCE_FILES: &[&str] = &[
    "Cargo.lock",
    "Cargo.toml",
    "deno.lock",
    "mise.lock",
    "mise.toml",
    "package.json",
    "tsconfig.json",
    "crates/gtl-contracts/Cargo.toml",
    "crates/gtl-web/Cargo.toml",
    "crates/gtl-web/Dioxus.toml",
    "crates/gtl-web/index.html",
    "xtask/src/verbs/dioxus_web.rs",
];
const SOURCE_DIRECTORIES: &[&str] = &[
    "crates/gtl-contracts/src",
    "crates/gtl-desktop/src",
    "crates/gtl-preview/src",
    "crates/gtl-web/assets",
    "crates/gtl-web/src",
    "frontend/diff",
    "frontend/diff-island",
    "frontend/shared",
];
const SOURCE_DIRECTORY_EXCLUSIONS: &[&str] = &["crates/gtl-preview/src/embedded"];
const GENERATED_SOURCE_OUTPUTS: &[&str] = &[
    "crates/gtl-web/assets/diff-island.css",
    "crates/gtl-web/assets/generated",
    "crates/gtl-web/assets/tailwind.css",
];
const FILE_COUNT_MAX: usize = 10_000;
const FILE_BYTES_MAX: u64 = 32 * 1024 * 1024;
const SOURCE_BYTES_MAX: u64 = 256 * 1024 * 1024;
const BUNDLE_FILE_BYTES_MAX: u64 = 64 * 1024 * 1024;
const BUNDLE_BYTES_MAX: u64 = 256 * 1024 * 1024;
const EXPECTED_BUNDLE_ASSETS: &[(&str, &str)] = &[
    ("app-icon-dxh", "ico"),
    ("diff-island-dxh", "css"),
    ("diff-island-dxh", "js"),
    ("focus-trap-dxh", "js"),
    ("gtl-web-dxh", "js"),
    ("gtl-web_bg-dxh", "wasm"),
    ("tailwind-dxh", "css"),
];
const TAILWIND_ARGUMENTS: &[&str] = &[
    "run",
    "--frozen",
    "--allow-all",
    "npm:@tailwindcss/cli@4.3.3",
    "--input",
    "crates/gtl-web/src/app/assets/styles/tailwind.css",
    "--output",
    "crates/gtl-web/assets/tailwind.css",
    "--minify",
];
const DIFF_ISLAND_STYLE_ARGUMENTS: &[&str] = &[
    "run",
    "--frozen",
    "--allow-all",
    "npm:@tailwindcss/cli@4.3.3",
    "--input",
    "crates/gtl-preview/src/styles/base.css",
    "--output",
    "crates/gtl-web/assets/diff-island.css",
    "--minify",
];
const BUNDLE_ARGUMENTS: &[&str] = &[
    "bundle",
    "--web",
    "--release",
    "--package",
    "gtl-web",
    "--locked",
];
const SERVE_ARGUMENTS: &[&str] = &[
    "serve",
    "--web",
    "--package",
    "gtl-web",
    "--locked",
    "--hot-reload",
    "true",
    "--watch",
    "true",
];
const DIFF_ISLAND_WATCH_ARGUMENTS: &[&str] = &["task", "--frozen", "dev:diff-island"];

pub(crate) fn run() -> Result<()> {
    build_release()?;
    process::result(Verb::WEB_BUILD, Status::Done);
    Ok(())
}

pub(crate) fn run_styles() -> Result<()> {
    build_styles()?;
    process::result(Verb::WEB_STYLES, Status::Done);
    Ok(())
}

pub(crate) fn serve(arguments: &[String]) -> Result<()> {
    frontend::build()?;
    build_styles()?;

    let root = project::repository_root();
    let _watchers = [
        DevelopmentWatcher::spawn(
            "dioxus-tailwind-watch",
            "deno",
            &watch_arguments(TAILWIND_ARGUMENTS),
            &root,
        )?,
        DevelopmentWatcher::spawn(
            "diff-island-tailwind-watch",
            "deno",
            &watch_arguments(DIFF_ISLAND_STYLE_ARGUMENTS),
            &root,
        )?,
        DevelopmentWatcher::spawn(
            "diff-island-script-watch",
            "deno",
            DIFF_ISLAND_WATCH_ARGUMENTS,
            &root,
        )?,
    ];
    process::run_step(
        &Step::new("dioxus-web-serve", "dx", SERVE_ARGUMENTS.iter().copied())
            .with_arguments(arguments.iter().cloned())
            .with_current_directory(root),
    )
}

fn watch_arguments(arguments: &[&'static str]) -> Vec<&'static str> {
    let mut watched = arguments.to_vec();
    watched.extend_from_slice(&["--watch=always", "--poll=100"]);
    watched
}

struct DevelopmentWatcher {
    label: &'static str,
    child: Child,
}

impl DevelopmentWatcher {
    fn spawn(label: &'static str, program: &str, arguments: &[&str], root: &Path) -> Result<Self> {
        let child = Command::new(program)
            .args(arguments)
            .current_dir(root)
            .spawn()
            .with_context(|| format!("start {label}"))?;
        Ok(Self { label, child })
    }
}

impl Drop for DevelopmentWatcher {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none()
            && let Err(error) = self.child.kill()
        {
            eprintln!("{0}: failed to stop watcher: {error}", self.label);
        }
        if let Err(error) = self.child.wait() {
            eprintln!("{0}: failed to reap watcher: {error}", self.label);
        }
    }
}

pub(crate) fn build_release() -> Result<()> {
    let root = project::repository_root();
    let _lock = project::lock_frontend_assets(&root)?;
    build_release_unlocked(&root)
}

pub(crate) fn build_release_unlocked(root: &Path) -> Result<()> {
    require_dioxus_cli()?;
    let inputs_before = release_input_fingerprint(root)?;
    frontend::build_unlocked(root)?;
    build_styles_unlocked(root)?;
    let target = project::cargo_target_directory(root)?;
    clean_release_outputs(root, &target)?;
    process::run_step(
        &Step::new("dioxus-web-release", "dx", BUNDLE_ARGUMENTS.iter().copied())
            .with_current_directory(root),
    )?;
    let inputs_after = release_input_fingerprint(root)?;
    ensure!(
        inputs_after == inputs_before,
        "Dioxus Web inputs changed during asset generation or bundling; retry the build"
    );
    verify_bundle_files(root)?;
    let fingerprint = source_fingerprint(root)?;
    let bundle = bundle_fingerprint(root)?;
    write_fingerprint(root, SOURCE_FINGERPRINT_PATH, &fingerprint)?;
    write_fingerprint(root, BUNDLE_FINGERPRINT_PATH, &bundle)?;
    verify_staged_bundle(root)
}

pub(crate) fn build_styles() -> Result<()> {
    let root = project::repository_root();
    let _lock = project::lock_frontend_assets(&root)?;
    build_styles_unlocked(&root)
}

pub(crate) fn build_styles_unlocked(root: &Path) -> Result<()> {
    which::which("deno").context("required tool `deno` is missing; run `mise install deno`")?;
    process::run_step(
        &Step::new(
            "dioxus-tailwind",
            "deno",
            TAILWIND_ARGUMENTS.iter().copied(),
        )
        .with_current_directory(root),
    )?;
    process::run_step(
        &Step::new(
            "diff-island-tailwind",
            "deno",
            DIFF_ISLAND_STYLE_ARGUMENTS.iter().copied(),
        )
        .with_current_directory(root),
    )
}

pub(crate) fn verify_staged_bundle_if_present() -> Result<()> {
    let root = project::repository_root();
    if root.join(DIST_DIRECTORY).exists() {
        verify_staged_bundle(&root)?;
    }
    Ok(())
}

pub(crate) fn verify_staged_bundle(root: &Path) -> Result<()> {
    verify_bundle_files(root)?;
    let marker_path = root.join(SOURCE_FINGERPRINT_PATH);
    let recorded = fs::read_to_string(&marker_path)
        .with_context(|| format!("read {}", marker_path.display()))?;
    let current = source_fingerprint(root)?;
    ensure!(
        recorded.trim() == current,
        "staged Dioxus Web bundle is stale; run `just web build`"
    );
    let bundle_marker = root.join(BUNDLE_FINGERPRINT_PATH);
    let recorded_bundle = fs::read_to_string(&bundle_marker)
        .with_context(|| format!("read {}", bundle_marker.display()))?;
    ensure!(
        recorded_bundle.trim() == bundle_fingerprint(root)?,
        "staged Dioxus Web assets changed after bundling; run `just web build`"
    );
    Ok(())
}

fn require_dioxus_cli() -> Result<()> {
    which::which("dx")
        .context("required tool `dx` is missing; run `mise install cargo:dioxus-cli`")?;
    let version = process::capture("dioxus-version", "dx", &["--version"])?;
    ensure!(
        dioxus_version_matches(&version),
        "required dx {DIOXUS_CLI_VERSION}, found `{}`; run `mise install cargo:dioxus-cli`",
        version.trim()
    );
    Ok(())
}

fn dioxus_version_matches(output: &str) -> bool {
    let mut fields = output.split_ascii_whitespace();
    fields.next() == Some("dioxus") && fields.next() == Some(DIOXUS_CLI_VERSION)
}

fn clean_release_outputs(root: &Path, target: &Path) -> Result<()> {
    let dist = root.join(DIST_DIRECTORY);
    let web_crate = root.join("crates/gtl-web");
    ensure!(
        dist.starts_with(&web_crate),
        "Dioxus staging path must remain inside crates/gtl-web"
    );
    if dist.exists() {
        fs::remove_dir_all(&dist).with_context(|| format!("remove {}", dist.display()))?;
    }

    let internal = target.join(DIOXUS_INTERNAL_RELEASE_DIRECTORY);
    let internal_owner = target.join("dx/gtl-web/release");
    ensure!(
        internal.starts_with(&internal_owner) && internal != internal_owner,
        "Dioxus internal release path must remain inside its package release directory"
    );
    if internal.exists() {
        fs::remove_dir_all(&internal).with_context(|| format!("remove {}", internal.display()))?;
    }
    Ok(())
}

fn verify_bundle_files(root: &Path) -> Result<()> {
    let public = root.join(PUBLIC_DIRECTORY);
    let index = public.join("index.html");
    ensure!(
        index.is_file() && index.metadata()?.len() > 0,
        "staged Dioxus Web bundle is missing a non-empty {}",
        index.display()
    );
    let files = collect_tree_files(&public)?;
    verify_bounded_bundle_files(&files, BUNDLE_FILE_BYTES_MAX, BUNDLE_BYTES_MAX)?;
    for extension in ["css", "js", "wasm"] {
        ensure!(
            files.iter().any(|path| {
                path.extension()
                    .is_some_and(|candidate| candidate == extension)
                    && path.metadata().is_ok_and(|metadata| metadata.len() > 0)
            }),
            "staged Dioxus Web bundle has no non-empty .{extension} asset"
        );
    }
    ensure!(
        files.len() == EXPECTED_BUNDLE_ASSETS.len() + 1,
        "staged Dioxus Web bundle contains unexpected or stale assets"
    );
    for (prefix, extension) in EXPECTED_BUNDLE_ASSETS {
        let matches = files
            .iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix))
                    && path.extension().is_some_and(|value| value == *extension)
            })
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "staged Dioxus Web bundle requires exactly one {prefix}*.{extension} asset"
        );
        let asset = matches[0];
        ensure!(
            asset.metadata()?.len() > 0,
            "staged Dioxus Web asset is empty: {}",
            asset.display()
        );
    }
    Ok(())
}

fn verify_bounded_bundle_files(files: &[PathBuf], file_max: u64, total_max: u64) -> Result<()> {
    let mut total = 0_u64;
    for path in files {
        let bytes = path
            .metadata()
            .with_context(|| format!("inspect staged asset {}", path.display()))?
            .len();
        ensure!(
            bytes <= file_max,
            "staged asset exceeds {file_max} bytes: {}",
            path.display()
        );
        total = total
            .checked_add(bytes)
            .context("staged asset byte count overflowed")?;
        ensure!(
            total <= total_max,
            "staged Dioxus Web bundle exceeds {total_max} bytes"
        );
    }
    Ok(())
}

fn write_fingerprint(root: &Path, relative: &str, fingerprint: &str) -> Result<()> {
    let marker_path = root.join(relative);
    let mut marker =
        File::create(&marker_path).with_context(|| format!("create {}", marker_path.display()))?;
    writeln!(marker, "{fingerprint}").with_context(|| format!("write {}", marker_path.display()))
}

fn source_fingerprint(root: &Path) -> Result<String> {
    fingerprint_sources(root, true)
}

fn release_input_fingerprint(root: &Path) -> Result<String> {
    fingerprint_sources(root, false)
}

fn fingerprint_sources(root: &Path, include_generated: bool) -> Result<String> {
    let mut files = SOURCE_FILES
        .iter()
        .map(|path| root.join(path))
        .collect::<Vec<_>>();
    let excluded_paths = SOURCE_DIRECTORY_EXCLUSIONS
        .iter()
        .map(|path| root.join(path))
        .collect::<Vec<_>>();
    for directory in SOURCE_DIRECTORIES {
        files.extend(
            collect_tree_files(&root.join(directory))?
                .into_iter()
                .filter(|path| {
                    excluded_paths
                        .iter()
                        .all(|excluded| !path.starts_with(excluded))
                }),
        );
    }
    files.sort();
    files.dedup();
    if !include_generated {
        let generated = GENERATED_SOURCE_OUTPUTS
            .iter()
            .map(|path| root.join(path))
            .collect::<Vec<_>>();
        files.retain(|path| generated.iter().all(|output| !path.starts_with(output)));
    }
    ensure!(
        files.len() <= FILE_COUNT_MAX,
        "Dioxus source inventory exceeds {FILE_COUNT_MAX} files"
    );

    let mut source_bytes = 0_u64;
    let mut digest = Sha256::new();
    for path in files {
        let metadata = path
            .metadata()
            .with_context(|| format!("inspect Dioxus source {}", path.display()))?;
        ensure!(
            metadata.is_file(),
            "Dioxus source is not a file: {}",
            path.display()
        );
        ensure!(
            metadata.len() <= FILE_BYTES_MAX,
            "Dioxus source exceeds {FILE_BYTES_MAX} bytes: {}",
            path.display()
        );
        source_bytes = source_bytes
            .checked_add(metadata.len())
            .context("Dioxus source byte count overflowed")?;
        ensure!(
            source_bytes <= SOURCE_BYTES_MAX,
            "Dioxus sources exceed {SOURCE_BYTES_MAX} bytes"
        );

        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("source escaped repository root: {}", path.display()))?;
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update([0]);
        let mut file =
            File::open(&path).with_context(|| format!("open Dioxus source {}", path.display()))?;
        let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
        loop {
            let count = file
                .read(&mut buffer)
                .with_context(|| format!("read Dioxus source {}", path.display()))?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        digest.update([0xff]);
    }
    let bytes = digest.finalize();
    let mut fingerprint = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut fingerprint, "{byte:02x}").context("encode Dioxus source fingerprint")?;
    }
    Ok(fingerprint)
}

fn bundle_fingerprint(root: &Path) -> Result<String> {
    let public = root.join(PUBLIC_DIRECTORY);
    let mut files = collect_tree_files(&public)?;
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(&public)
            .with_context(|| format!("staged asset escaped public root: {}", path.display()))?;
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update([0]);
        let mut file =
            File::open(&path).with_context(|| format!("open staged asset {}", path.display()))?;
        let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
        loop {
            let count = file
                .read(&mut buffer)
                .with_context(|| format!("read staged asset {}", path.display()))?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        digest.update([0xff]);
    }
    let bytes = digest.finalize();
    let mut fingerprint = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut fingerprint, "{byte:02x}").context("encode Dioxus bundle fingerprint")?;
    }
    Ok(fingerprint)
}

fn collect_tree_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut directories = vec![directory.to_path_buf()];
    let mut files = Vec::new();
    let mut entries_seen = 0_usize;
    while let Some(current) = directories.pop() {
        let entries = fs::read_dir(&current)
            .with_context(|| format!("read directory {}", current.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("read entry in {}", current.display()))?;
            entries_seen += 1;
            ensure!(
                entries_seen <= FILE_COUNT_MAX,
                "file inventory under {} exceeds {FILE_COUNT_MAX} entries",
                directory.display()
            );
            let file_type = entry
                .file_type()
                .with_context(|| format!("inspect {}", entry.path().display()))?;
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path());
            } else {
                bail!(
                    "unsupported staged or source entry: {}",
                    entry.path().display()
                );
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) {
        for relative in SOURCE_FILES {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("fixture file parent"))
                .expect("fixture file parent is writable");
            fs::write(path, relative).expect("fixture source is writable");
        }
        for directory in SOURCE_DIRECTORIES {
            let path = root.join(directory).join("fixture.txt");
            fs::create_dir_all(path.parent().expect("fixture directory"))
                .expect("fixture directory is writable");
            fs::write(path, directory).expect("fixture directory source is writable");
        }
        for excluded in SOURCE_DIRECTORY_EXCLUSIONS {
            fs::create_dir_all(root.join(excluded)).expect("excluded directory is writable");
        }
    }

    fn stage_fresh_bundle(root: &Path) {
        let public = root.join(PUBLIC_DIRECTORY);
        fs::create_dir_all(&public).expect("public bundle directory is writable");
        for (name, contents) in [
            ("index.html", "<html></html>"),
            ("assets/app-icon-dxhone.ico", "icon"),
            ("assets/diff-island-dxhone.css", "host{}"),
            ("assets/diff-island-dxhone.js", "island"),
            ("assets/focus-trap-dxhone.js", "focus"),
            ("assets/gtl-web-dxhone.js", "app"),
            ("assets/gtl-web_bg-dxhone.wasm", "wasm"),
            ("assets/tailwind-dxhone.css", "body{}"),
        ] {
            let path = public.join(name);
            fs::create_dir_all(path.parent().expect("bundle asset parent"))
                .expect("bundle asset parent is writable");
            fs::write(path, contents).expect("bundle asset is writable");
        }
        let fingerprint = source_fingerprint(root).expect("fixture fingerprint");
        let bundle = bundle_fingerprint(root).expect("fixture bundle fingerprint");
        write_fingerprint(root, SOURCE_FINGERPRINT_PATH, &fingerprint)
            .expect("fixture source marker is writable");
        write_fingerprint(root, BUNDLE_FINGERPRINT_PATH, &bundle)
            .expect("fixture bundle marker is writable");
    }

    #[test]
    fn staged_bundle_requires_every_runtime_asset_kind() {
        let root = tempfile::tempdir().expect("temporary repository");
        fixture(root.path());
        let public = root.path().join(PUBLIC_DIRECTORY);
        fs::create_dir_all(&public).expect("public bundle directory is writable");
        fs::write(public.join("index.html"), "<html></html>").expect("bundle index is writable");

        let error = verify_bundle_files(root.path())
            .expect_err("bundle without runtime assets must fail")
            .to_string();

        assert!(error.contains("has no non-empty .css asset"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_a_missing_distribution() {
        let root = tempfile::tempdir().expect("temporary repository");
        fixture(root.path());

        let error = verify_staged_bundle(root.path())
            .expect_err("missing distribution must fail")
            .to_string();

        assert!(error.contains("missing a non-empty"), "{error}");
        assert!(error.contains("dist/public/index.html"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_source_drift() {
        let root = tempfile::tempdir().expect("temporary repository");
        fixture(root.path());
        stage_fresh_bundle(root.path());
        fs::write(
            root.path().join("crates/gtl-web/src/fixture.txt"),
            "changed",
        )
        .expect("fixture source changes");

        let error = verify_staged_bundle(root.path())
            .expect_err("changed sources must stale the bundle")
            .to_string();

        assert!(error.contains("bundle is stale"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_local_contract_drift() {
        let root = tempfile::tempdir().expect("temporary repository");
        fixture(root.path());
        stage_fresh_bundle(root.path());
        fs::write(
            root.path().join("crates/gtl-contracts/src/fixture.txt"),
            "changed",
        )
        .expect("fixture contract changes");

        let error = verify_staged_bundle(root.path())
            .expect_err("changed local dependency must stale the bundle")
            .to_string();

        assert!(error.contains("bundle is stale"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_a_stale_hashed_asset() {
        let root = tempfile::tempdir().expect("temporary repository");
        fixture(root.path());
        stage_fresh_bundle(root.path());
        fs::write(
            root.path()
                .join(PUBLIC_DIRECTORY)
                .join("assets/gtl-web-dxhold.js"),
            "stale",
        )
        .expect("stale bundle asset is writable");

        let error = verify_bundle_files(root.path())
            .expect_err("stale hashed asset must fail")
            .to_string();

        assert!(error.contains("unexpected or stale assets"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_oversized_files_and_aggregate_output() {
        let root = tempfile::tempdir().expect("temporary directory");
        let first = root.path().join("first.js");
        let second = root.path().join("second.wasm");
        fs::write(&first, "12345").expect("first asset is writable");
        fs::write(&second, "12345").expect("second asset is writable");

        let file_error = verify_bounded_bundle_files(std::slice::from_ref(&first), 4, 10)
            .expect_err("oversized staged asset must fail")
            .to_string();
        assert!(file_error.contains("exceeds 4 bytes"), "{file_error}");

        let total_error = verify_bounded_bundle_files(&[first, second], 5, 9)
            .expect_err("oversized staged aggregate must fail")
            .to_string();
        assert!(
            total_error.contains("bundle exceeds 9 bytes"),
            "{total_error}"
        );
    }

    #[test]
    fn source_fingerprint_ignores_generated_embedded_outputs() {
        let root = tempfile::tempdir().expect("temporary repository");
        fixture(root.path());
        let fingerprint_before = source_fingerprint(root.path()).expect("fixture fingerprint");
        let embedded = root
            .path()
            .join("crates/gtl-preview/src/embedded/generated/preview.css");
        fs::create_dir_all(embedded.parent().expect("embedded parent"))
            .expect("embedded parent is writable");
        fs::write(embedded, "generated").expect("embedded output is writable");

        assert_eq!(
            source_fingerprint(root.path()).expect("fixture fingerprint"),
            fingerprint_before
        );
    }

    #[test]
    fn clean_release_outputs_remove_staged_and_internal_old_assets() {
        let root = tempfile::tempdir().expect("temporary repository");
        let target = root.path().join("target");
        let staged = root.path().join(PUBLIC_DIRECTORY).join("old.js");
        let internal = target
            .join(DIOXUS_INTERNAL_RELEASE_DIRECTORY)
            .join("public/assets/old.js");
        let neighbor = target.join("dx/other-package/keep");
        for path in [&staged, &internal, &neighbor] {
            fs::create_dir_all(path.parent().expect("old asset parent"))
                .expect("old asset parent is writable");
            fs::write(path, "old").expect("old asset is writable");
        }

        clean_release_outputs(root.path(), &target).expect("release outputs clean");

        assert!(!root.path().join(DIST_DIRECTORY).exists());
        assert!(!target.join(DIOXUS_INTERNAL_RELEASE_DIRECTORY).exists());
        assert!(
            neighbor.exists(),
            "unrelated target output must be preserved"
        );
    }

    #[test]
    fn release_commands_are_locked_and_write_the_crate_owned_inputs() {
        assert_eq!(
            BUNDLE_ARGUMENTS,
            [
                "bundle",
                "--web",
                "--release",
                "--package",
                "gtl-web",
                "--locked"
            ]
        );
        assert_eq!(TAILWIND_ARGUMENTS[3], "npm:@tailwindcss/cli@4.3.3");
        assert!(TAILWIND_ARGUMENTS.contains(&"crates/gtl-web/assets/tailwind.css"));
        assert_eq!(
            DIFF_ISLAND_STYLE_ARGUMENTS[5],
            "crates/gtl-preview/src/styles/base.css"
        );
        assert!(DIFF_ISLAND_STYLE_ARGUMENTS.contains(&"crates/gtl-web/assets/diff-island.css"));
    }

    #[test]
    fn development_serve_watches_both_stylesheets_and_the_island_script() {
        for arguments in [TAILWIND_ARGUMENTS, DIFF_ISLAND_STYLE_ARGUMENTS] {
            let watched = watch_arguments(arguments);
            assert!(watched.ends_with(&["--watch=always", "--poll=100"]));
        }
        assert_eq!(
            DIFF_ISLAND_WATCH_ARGUMENTS,
            ["task", "--frozen", "dev:diff-island"]
        );
        assert!(
            SERVE_ARGUMENTS
                .windows(2)
                .any(|pair| pair == ["--watch", "true"])
        );
        assert!(
            SERVE_ARGUMENTS
                .windows(2)
                .any(|pair| pair == ["--hot-reload", "true"])
        );
    }

    #[test]
    fn dioxus_version_requires_the_exact_pinned_release() {
        assert!(dioxus_version_matches("dioxus 0.7.10 (57d6794)"));
        assert!(!dioxus_version_matches("dioxus 0.7.100 (future)"));
        assert!(!dioxus_version_matches("other 0.7.10"));
    }
}
