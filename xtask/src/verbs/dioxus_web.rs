//! Deterministic Dioxus Web release staging for the Tauri viewer.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use sha2::{Digest, Sha256};

use super::{cargo_target_directory, lock_web_assets, repository_root};
use crate::{process, task::Step};

const DIST_DIRECTORY: &str = "crates/gtl-web/dist";
const PUBLIC_DIRECTORY: &str = "crates/gtl-web/dist/public";
const SOURCE_FINGERPRINT_PATH: &str = "crates/gtl-web/dist/.source-fingerprint";
const BUNDLE_FINGERPRINT_PATH: &str = "crates/gtl-web/dist/.bundle-fingerprint";
const DESKTOP_INTERNAL_RELEASE_DIRECTORY: &str = "dx/gtl-web/release/web";
const SOURCE_FILES: &[&str] = &[
    "Cargo.lock",
    "Cargo.toml",
    "deno.json",
    "deno.lock",
    "mise.lock",
    "mise.toml",
    "crates/gtl-wire/Cargo.toml",
    "crates/gtl-parser/Cargo.toml",
    "crates/gtl-web/Cargo.toml",
    "crates/gtl-web/Dioxus.toml",
    "crates/gtl-web/index.html",
    "crates/gtl-web-contracts/Cargo.toml",
    "xtask/src/verbs/dioxus_web.rs",
];
const SOURCE_DIRECTORIES: &[&str] = &[
    "crates/gtl-wire/src",
    "crates/gtl-desktop/src",
    "crates/gtl-artifacts/src",
    "crates/gtl-parser/src",
    "crates/gtl-web/assets",
    "crates/gtl-web/src",
    "crates/gtl-web-contracts/src",
];
const GENERATED_SOURCE_OUTPUTS: &[&str] = &["crates/gtl-web/assets/tailwind.css"];
const FILE_COUNT_MAX: usize = 10_000;
const FILE_BYTES_MAX: u64 = 32 * 1024 * 1024;
const SOURCE_BYTES_MAX: u64 = 256 * 1024 * 1024;
const BUNDLE_FILE_BYTES_MAX: u64 = 64 * 1024 * 1024;
const BUNDLE_BYTES_MAX: u64 = 256 * 1024 * 1024;
const EXPECTED_BUNDLE_ASSETS: &[(&str, &str)] = &[
    ("app-icon-dxh", "ico"),
    ("focus-trap-dxh", "js"),
    ("gtl-web-dxh", "js"),
    ("gtl-web_bg-dxh", "wasm"),
    ("tailwind-dxh", "css"),
];
const TAILWIND_ARGUMENTS: &[&str] = &[
    "run",
    "--frozen",
    "--allow-all",
    "@tailwindcss/cli",
    "--input",
    "crates/gtl-web/src/app/assets/styles/tailwind.css",
    "--output",
    "crates/gtl-web/assets/tailwind.css",
    "--minify",
];
const DESKTOP_BUNDLE_ARGUMENTS: &[&str] = &[
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
const DEVELOPMENT_RUST_SOURCE_DIRECTORIES: &[&str] = &[
    "crates/gtl-wire/src",
    "crates/gtl-parser/src",
    "crates/gtl-web-contracts/src",
    "crates/gtl-web/src",
];
const DEVELOPMENT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEVELOPMENT_STOP_GRACE_PERIOD: Duration = Duration::from_secs(2);

pub(crate) fn serve(arguments: &[String]) -> Result<()> {
    build_styles()?;

    let root = repository_root();
    let _tailwind_watcher = DevelopmentWatcher::spawn(
        "dioxus-tailwind-watch",
        "deno",
        &watch_arguments(TAILWIND_ARGUMENTS),
        &root,
    )?;
    let step = super::wasm_c::configure_step(&root, development_serve_step(&root, arguments))?;
    run_development_server(&step, &root)
}

fn development_serve_step(root: &Path, arguments: &[String]) -> Step {
    Step::new("dioxus-web-serve", "dx", SERVE_ARGUMENTS.iter().copied())
        .with_arguments(arguments.iter().cloned())
        .with_environment("CARGO_INCREMENTAL", "1")
        .with_environment("RUSTC_WRAPPER", "")
        .with_current_directory(root)
}

fn run_development_server(step: &Step, root: &Path) -> Result<()> {
    // Dioxus 0.7 snapshots its Rust source map at startup and silently ignores files created
    // afterward. Restarting only for new Rust paths refreshes that map without sacrificing normal
    // RSX hot reloads.
    let mut known_sources = development_rust_sources(root)?;
    while let Some(current_sources) =
        run_development_server_until_source_change(step, root, &known_sources)?
    {
        known_sources = current_sources;
    }
    Ok(())
}

fn run_development_server_until_source_change(
    step: &Step,
    root: &Path,
    known_sources: &BTreeSet<PathBuf>,
) -> Result<Option<BTreeSet<PathBuf>>> {
    let mut server = DevelopmentServer::spawn(step)?;
    loop {
        if let Some(status) = server.try_wait()? {
            development_server_result(step, status)?;
            return Ok(None);
        }

        let current_sources = development_rust_sources(root)?;
        let new_sources = new_development_rust_sources(known_sources, &current_sources);
        if new_sources.is_empty() {
            thread::sleep(DEVELOPMENT_POLL_INTERVAL);
            continue;
        }

        let paths = new_sources
            .iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("dioxus-web-serve: restarting Dioxus to register new Rust source: {paths}");
        server.stop()?;
        return Ok(Some(current_sources));
    }
}

fn development_server_result(step: &Step, status: ExitStatus) -> Result<()> {
    ensure!(
        status.success(),
        "{} failed (exit {})",
        step.label(),
        status.code().unwrap_or(-1)
    );
    Ok(())
}

fn development_rust_sources(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut sources = BTreeSet::new();
    for directory in DEVELOPMENT_RUST_SOURCE_DIRECTORIES {
        sources.extend(
            collect_tree_files(&root.join(directory))?
                .into_iter()
                .filter(|path| path.extension().is_some_and(|extension| extension == "rs")),
        );
    }
    ensure!(
        sources.len() <= FILE_COUNT_MAX,
        "Dioxus development source inventory exceeds {FILE_COUNT_MAX} Rust files"
    );
    Ok(sources)
}

fn new_development_rust_sources(
    known: &BTreeSet<PathBuf>,
    current: &BTreeSet<PathBuf>,
) -> Vec<PathBuf> {
    current.difference(known).cloned().collect()
}

struct DevelopmentServer {
    label: String,
    child: Child,
}

impl DevelopmentServer {
    fn spawn(step: &Step) -> Result<Self> {
        Ok(Self {
            label: step.label().to_owned(),
            child: process::spawn_step(step)?,
        })
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.child
            .try_wait()
            .with_context(|| format!("poll {} process", self.label))
    }

    fn stop(&mut self) -> Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }

        self.request_stop()?;
        let deadline = Instant::now() + DEVELOPMENT_STOP_GRACE_PERIOD;
        while Instant::now() < deadline && self.try_wait()?.is_none() {
            thread::sleep(DEVELOPMENT_POLL_INTERVAL);
        }
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        if let Err(error) = self.child.kill()
            && self.try_wait()?.is_none()
        {
            return Err(error).context(format!("kill {} process", self.label));
        }
        self.child
            .wait()
            .with_context(|| format!("reap {} process", self.label))?;
        Ok(())
    }

    #[cfg(unix)]
    fn request_stop(&mut self) -> Result<()> {
        if let Err(error) = self.child.signal(Signal::SIGINT)
            && self.try_wait()?.is_none()
        {
            return Err(error).context(format!("stop {} process", self.label));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn request_stop(&mut self) -> Result<()> {
        if let Err(error) = self.child.kill()
            && self.try_wait()?.is_none()
        {
            return Err(error).context(format!("stop {} process", self.label));
        }
        Ok(())
    }
}

impl Drop for DevelopmentServer {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!(
                "{}: failed to stop development server: {error:#}",
                self.label
            );
        }
    }
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
    let root = repository_root();
    let _lock = lock_web_assets(&root)?;
    build_release_unlocked(&root)
}

pub(crate) fn build_release_unlocked(root: &Path) -> Result<()> {
    let inputs_before = release_input_fingerprint(root)?;
    build_artifact_styles_unlocked(root)?;
    let target = cargo_target_directory(root)?;
    clean_desktop_release_outputs(root, &target)?;
    let step = Step::new(
        "dioxus-web-release",
        "dx",
        DESKTOP_BUNDLE_ARGUMENTS.iter().copied(),
    )
    .with_environment("RUSTC_WRAPPER", "")
    .with_current_directory(root);
    process::run_step(&super::wasm_c::configure_step(root, step)?)?;
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

pub(crate) fn build_artifact_styles_unlocked(root: &Path) -> Result<()> {
    let inputs_before = release_input_fingerprint(root)?;
    build_styles_unlocked(root)?;
    ensure!(
        release_input_fingerprint(root)? == inputs_before,
        "static artifact inputs changed during stylesheet generation; retry the build"
    );
    Ok(())
}

pub(crate) fn build_styles() -> Result<()> {
    let root = repository_root();
    let _lock = lock_web_assets(&root)?;
    build_styles_unlocked(&root)
}

pub(crate) fn build_styles_unlocked(root: &Path) -> Result<()> {
    process::run_step(
        &Step::new(
            "dioxus-tailwind",
            "deno",
            TAILWIND_ARGUMENTS.iter().copied(),
        )
        .with_current_directory(root),
    )
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

fn clean_desktop_release_outputs(root: &Path, target: &Path) -> Result<()> {
    let dist = root.join(DIST_DIRECTORY);
    let web_crate = root.join("crates/gtl-web");
    ensure!(
        dist.starts_with(&web_crate),
        "Dioxus staging path must remain inside crates/gtl-web"
    );
    if dist.exists() {
        fs::remove_dir_all(&dist).with_context(|| format!("remove {}", dist.display()))?;
    }

    let internal = target.join(DESKTOP_INTERNAL_RELEASE_DIRECTORY);
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
    for directory in SOURCE_DIRECTORIES {
        files.extend(collect_tree_files(&root.join(directory))?);
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
        update_digest_from_file(&mut digest, &path, "Dioxus source")?;
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
        update_digest_from_file(&mut digest, &path, "staged asset")?;
        digest.update([0xff]);
    }
    let bytes = digest.finalize();
    let mut fingerprint = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut fingerprint, "{byte:02x}").context("encode Dioxus bundle fingerprint")?;
    }
    Ok(fingerprint)
}

fn update_digest_from_file(digest: &mut Sha256, path: &Path, description: &str) -> Result<()> {
    let mut file =
        File::open(path).with_context(|| format!("open {description} {}", path.display()))?;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut count = read_digest_chunk(&mut file, &mut buffer, path, description)?;
    while count > 0 {
        digest.update(&buffer[..count]);
        count = read_digest_chunk(&mut file, &mut buffer, path, description)?;
    }
    Ok(())
}

fn read_digest_chunk(
    file: &mut File,
    buffer: &mut [u8],
    path: &Path,
    description: &str,
) -> Result<usize> {
    file.read(buffer)
        .with_context(|| format!("read {description} {}", path.display()))
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
            collect_tree_entry(&entry, &mut directories, &mut files)?;
        }
    }
    Ok(files)
}

fn collect_tree_entry(
    entry: &fs::DirEntry,
    directories: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = entry.path();
    let file_type = entry
        .file_type()
        .with_context(|| format!("inspect {}", path.display()))?;
    if file_type.is_dir() {
        directories.push(path);
        return Ok(());
    }
    ensure!(
        file_type.is_file(),
        "unsupported staged or source entry: {}",
        path.display()
    );
    files.push(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) {
        for relative in SOURCE_FILES {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative).unwrap();
        }
        for directory in SOURCE_DIRECTORIES {
            let path = root.join(directory).join("fixture.txt");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, directory).unwrap();
        }
    }

    fn stage_fresh_bundle(root: &Path) {
        let public = root.join(PUBLIC_DIRECTORY);
        fs::create_dir_all(&public).unwrap();
        for (name, contents) in [
            ("index.html", "<html></html>"),
            ("assets/app-icon-dxhone.ico", "icon"),
            ("assets/focus-trap-dxhone.js", "focus"),
            ("assets/gtl-web-dxhone.js", "app"),
            ("assets/gtl-web_bg-dxhone.wasm", "wasm"),
            ("assets/tailwind-dxhone.css", "body{}"),
        ] {
            let path = public.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let fingerprint = source_fingerprint(root).unwrap();
        let bundle = bundle_fingerprint(root).unwrap();
        write_fingerprint(root, SOURCE_FINGERPRINT_PATH, &fingerprint).unwrap();
        write_fingerprint(root, BUNDLE_FINGERPRINT_PATH, &bundle).unwrap();
    }

    #[test]
    fn staged_bundle_requires_every_runtime_asset_kind() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        let public = root.path().join(PUBLIC_DIRECTORY);
        fs::create_dir_all(&public).unwrap();
        fs::write(public.join("index.html"), "<html></html>").unwrap();

        let error = verify_bundle_files(root.path()).unwrap_err().to_string();

        assert!(error.contains("has no non-empty .css asset"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_a_missing_distribution() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());

        let error = verify_staged_bundle(root.path()).unwrap_err().to_string();

        assert!(error.contains("missing a non-empty"), "{error}");
        assert!(error.contains("dist/public/index.html"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_source_drift() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        stage_fresh_bundle(root.path());
        fs::write(
            root.path().join("crates/gtl-web/src/fixture.txt"),
            "changed",
        )
        .unwrap();

        let error = verify_staged_bundle(root.path()).unwrap_err().to_string();

        assert!(error.contains("bundle is stale"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_local_contract_drift() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        stage_fresh_bundle(root.path());
        fs::write(
            root.path().join("crates/gtl-wire/src/fixture.txt"),
            "changed",
        )
        .unwrap();

        let error = verify_staged_bundle(root.path()).unwrap_err().to_string();

        assert!(error.contains("bundle is stale"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_a_stale_hashed_asset() {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        stage_fresh_bundle(root.path());
        fs::write(
            root.path()
                .join(PUBLIC_DIRECTORY)
                .join("assets/gtl-web-dxhold.js"),
            "stale",
        )
        .unwrap();

        let error = verify_bundle_files(root.path()).unwrap_err().to_string();

        assert!(error.contains("unexpected or stale assets"), "{error}");
    }

    #[test]
    fn staged_bundle_rejects_oversized_files_and_aggregate_output() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.js");
        let second = root.path().join("second.wasm");
        fs::write(&first, "12345").unwrap();
        fs::write(&second, "12345").unwrap();

        let file_error = verify_bounded_bundle_files(std::slice::from_ref(&first), 4, 10)
            .unwrap_err()
            .to_string();
        assert!(file_error.contains("exceeds 4 bytes"), "{file_error}");

        let total_error = verify_bounded_bundle_files(&[first, second], 5, 9)
            .unwrap_err()
            .to_string();
        assert!(
            total_error.contains("bundle exceeds 9 bytes"),
            "{total_error}"
        );
    }

    #[test]
    fn release_output_cleanup_is_bounded_to_the_desktop_dioxus_application() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let staged = root.path().join(PUBLIC_DIRECTORY).join("old.js");
        let desktop_internal = target
            .join(DESKTOP_INTERNAL_RELEASE_DIRECTORY)
            .join("public/assets/old.js");
        let neighbor = target.join("dx/other-package/keep");
        for path in [&staged, &desktop_internal, &neighbor] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "old").unwrap();
        }

        clean_desktop_release_outputs(root.path(), &target).unwrap();

        assert!(!root.path().join(DIST_DIRECTORY).exists());
        assert!(!target.join(DESKTOP_INTERNAL_RELEASE_DIRECTORY).exists());
        assert!(
            neighbor.exists(),
            "unrelated target output must be preserved"
        );
    }

    #[test]
    fn release_commands_are_locked_and_write_the_crate_owned_inputs() {
        assert_eq!(
            DESKTOP_BUNDLE_ARGUMENTS,
            [
                "bundle",
                "--web",
                "--release",
                "--package",
                "gtl-web",
                "--locked"
            ]
        );
        assert_eq!(TAILWIND_ARGUMENTS[3], "@tailwindcss/cli");
        assert!(TAILWIND_ARGUMENTS.contains(&"crates/gtl-web/assets/tailwind.css"));
    }

    #[test]
    fn development_serve_watches_the_shared_tailwind_source() {
        let watched = watch_arguments(TAILWIND_ARGUMENTS);
        assert!(watched.ends_with(&["--watch=always", "--poll=100"]));
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
    fn development_serve_owns_the_incremental_compile_environment() {
        let root = tempfile::tempdir().unwrap();
        let forwarded = ["--port".to_owned(), "8081".to_owned()];

        let step = development_serve_step(root.path(), &forwarded);

        assert_eq!(step.program(), "dx");
        assert!(step.arguments().ends_with(&forwarded));
        assert_eq!(step.current_directory(), Some(root.path()));
        assert!(
            step.environment()
                .contains(&("CARGO_INCREMENTAL".to_owned(), "1".to_owned()))
        );
        assert!(
            step.environment()
                .contains(&("RUSTC_WRAPPER".to_owned(), String::new()))
        );
    }

    #[test]
    fn development_source_inventory_exposes_only_new_rust_sources() {
        let root = tempfile::tempdir().unwrap();
        for directory in DEVELOPMENT_RUST_SOURCE_DIRECTORIES {
            fs::create_dir_all(root.path().join(directory)).unwrap();
        }
        let before = development_rust_sources(root.path()).unwrap();
        let rust_source = root
            .path()
            .join("crates/gtl-web/src/shared/ui/code_text.rs");
        fs::create_dir_all(rust_source.parent().unwrap()).unwrap();
        fs::write(&rust_source, "pub fn code_text() {}").unwrap();
        fs::write(root.path().join("crates/gtl-web/src/notes.txt"), "not Rust").unwrap();
        let after = development_rust_sources(root.path()).unwrap();

        assert_eq!(new_development_rust_sources(&before, &after), [rust_source]);
    }
}
