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

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use flate2::{Compression, GzBuilder};
use sha2::{Digest, Sha256};

use super::{cargo_target_directory, lock_web_assets, repository_root};
use crate::{process, task::Step};

const DIST_DIRECTORY: &str = "crates/gtl-web/dist";
const PUBLIC_DIRECTORY: &str = "crates/gtl-web/dist/public";
const SOURCE_FINGERPRINT_PATH: &str = "crates/gtl-web/dist/.source-fingerprint";
const BUNDLE_FINGERPRINT_PATH: &str = "crates/gtl-web/dist/.bundle-fingerprint";
const DESKTOP_INTERNAL_RELEASE_DIRECTORY: &str = "dx/gtl-web/release/web";
const ARTIFACT_PROFILE: &str = "artifact-web-release";
const ARTIFACT_INTERNAL_RELEASE_DIRECTORY: &str = "dx/gtl-artifact/release/web";
const ARTIFACT_RUNTIME_CACHE_DIRECTORY: &str = "target/generated/gtl-artifacts";
const ARTIFACT_RUNTIME_ASSET_ID: &str = "gtl-artifact-runtime";
const ARTIFACT_RUNTIME_MAX_BYTES: usize = 16 * 1024 * 1024;
const ARTIFACT_ASSET_LOADER: &str = r#"
function gtlArtifactAssetError(code, id, cause) {
    const error = new Error(`${code}: ${id}`);
    error.name = "GtlArtifactAssetError";
    error.code = code;
    error.assetId = id;
    if (cause !== undefined) error.cause = cause;
    return error;
}

function gtlDecodeBase64(encoded, id) {
    if (encoded.length === 0 || encoded.length % 4 !== 0 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)) {
        throw gtlArtifactAssetError("INVALID_BASE64", id);
    }
    const chunks = [];
    for (let offset = 0; offset < encoded.length; offset += 32768) {
        let decoded;
        try {
            decoded = atob(encoded.slice(offset, offset + 32768));
        } catch (cause) {
            throw gtlArtifactAssetError("INVALID_BASE64", id, cause);
        }
        const bytes = new Uint8Array(decoded.length);
        for (let index = 0; index < decoded.length; index += 1) bytes[index] = decoded.charCodeAt(index);
        chunks.push(bytes);
    }
    return chunks;
}

globalThis.__gtlLoadCompressedAsset = async function (id, maxBytes, removeAfterRead = false) {
    if (typeof globalThis.DecompressionStream !== "function") {
        throw gtlArtifactAssetError("UNAVAILABLE", id);
    }
    const node = document.getElementById(id);
    if (node === null) throw gtlArtifactAssetError("MISSING", id);
    if (node.dataset.encoding !== "base64" || node.dataset.compression !== "gzip") {
        throw gtlArtifactAssetError("INVALID_METADATA", id);
    }
    const expectedText = node.dataset.uncompressedBytes;
    if (expectedText === undefined || !/^(?:0|[1-9][0-9]*)$/.test(expectedText)) {
        throw gtlArtifactAssetError("INVALID_METADATA", id);
    }
    const expectedBytes = Number(expectedText);
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 0 || !Number.isSafeInteger(expectedBytes)) {
        throw gtlArtifactAssetError("INVALID_METADATA", id);
    }
    if (expectedBytes > maxBytes) throw gtlArtifactAssetError("TOO_LARGE", id);
    const encoded = (node.textContent ?? "").trim();
    const encodedLimit = Math.ceil((maxBytes + 65536) / 3) * 4;
    if (encoded.length > encodedLimit) throw gtlArtifactAssetError("TOO_LARGE", id);
    const compressedChunks = gtlDecodeBase64(encoded, id);
    let reader;
    try {
        reader = new Blob(compressedChunks)
            .stream()
            .pipeThrough(new DecompressionStream("gzip"))
            .getReader();
    } catch (cause) {
        throw gtlArtifactAssetError("INVALID_GZIP", id, cause);
    }
    const outputChunks = [];
    let outputBytes = 0;
    try {
        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            outputBytes += value.byteLength;
            if (outputBytes > maxBytes || outputBytes > expectedBytes) {
                await reader.cancel();
                throw gtlArtifactAssetError("TOO_LARGE", id);
            }
            outputChunks.push(value);
        }
    } catch (cause) {
        if (cause?.name === "GtlArtifactAssetError") throw cause;
        throw gtlArtifactAssetError("INVALID_GZIP", id, cause);
    } finally {
        reader.releaseLock();
    }
    if (outputBytes !== expectedBytes) throw gtlArtifactAssetError("INVALID_GZIP", id);
    const output = new Uint8Array(outputBytes);
    let offset = 0;
    for (const chunk of outputChunks) {
        output.set(chunk, offset);
        offset += chunk.byteLength;
    }
    if (removeAfterRead) node.remove();
    return output;
};

globalThis.__gtlShowArtifactInitializationError = function (error) {
    let target = document.getElementById("main");
    if (target === null) {
        target = document.createElement("main");
        document.body.append(target);
    }
    const messages = {
        UNAVAILABLE: "This browser cannot decompress this offline diff artifact.",
        MISSING: "This diff artifact is missing its compressed runtime.",
        INVALID_METADATA: "The embedded artifact runtime has invalid size metadata.",
        INVALID_BASE64: "The embedded artifact runtime is not valid base64.",
        INVALID_GZIP: "The embedded artifact runtime gzip stream is corrupt.",
        TOO_LARGE: "The embedded artifact runtime exceeds its declared size limit.",
    };
    target.replaceChildren();
    target.className = "grid h-screen place-content-center bg-bg px-5 text-center text-ink";
    target.setAttribute("role", "alert");
    const heading = document.createElement("h1");
    heading.className = "font-semibold";
    heading.textContent = "Unable to open diff artifact";
    const detail = document.createElement("p");
    detail.className = "mt-1 text-ink-2";
    detail.textContent = messages[error?.code] ?? "The embedded artifact runtime is corrupt or incompatible.";
    target.append(heading, detail);
};
"#;
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
const ARTIFACT_BUILD_ARGUMENTS: &[&str] = &[
    "build",
    "--web",
    "--profile",
    ARTIFACT_PROFILE,
    "--package",
    "gtl-web",
    "--bin",
    "gtl-artifact",
    "--no-default-features",
    "--features",
    "artifact",
    "--inject-loading-scripts",
    "false",
    "--debug-symbols",
    "false",
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
    loop {
        let mut server = DevelopmentServer::spawn(step)?;
        loop {
            if let Some(status) = server.try_wait()? {
                return development_server_result(step, status);
            }

            let current_sources = development_rust_sources(root)?;
            let new_sources = new_development_rust_sources(&known_sources, &current_sources);
            if !new_sources.is_empty() {
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
                eprintln!(
                    "dioxus-web-serve: restarting Dioxus to register new Rust source: {paths}"
                );
                server.stop()?;
                known_sources = current_sources;
                break;
            }
            thread::sleep(DEVELOPMENT_POLL_INTERVAL);
        }
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
        for path in collect_tree_files(&root.join(directory))? {
            if path.extension().is_some_and(|extension| extension == "rs") {
                sources.insert(path);
            }
        }
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
        loop {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                if let Err(error) = self.child.kill()
                    && self.try_wait()?.is_none()
                {
                    return Err(error).context(format!("kill {} process", self.label));
                }
                self.child
                    .wait()
                    .with_context(|| format!("reap {} process", self.label))?;
                return Ok(());
            }
            thread::sleep(DEVELOPMENT_POLL_INTERVAL);
        }
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
    build_artifact_assets_unlocked(root)?;
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

pub(crate) fn build_artifact_assets_unlocked(root: &Path) -> Result<()> {
    let inputs_before = release_input_fingerprint(root)?;
    build_styles_unlocked(root)?;
    let target = cargo_target_directory(root)?;
    build_artifact_runtime_unlocked(root, &target)?;
    ensure!(
        release_input_fingerprint(root)? == inputs_before,
        "offline artifact inputs changed during asset generation; retry the build"
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

fn build_artifact_runtime_unlocked(root: &Path, target: &Path) -> Result<()> {
    verify_artifact_release_profile(root)?;
    clean_artifact_release_output(target)?;
    let step = Step::new(
        "dioxus-artifact-release",
        "dx",
        ARTIFACT_BUILD_ARGUMENTS.iter().copied(),
    )
    .with_environment("RUSTC_WRAPPER", "")
    .with_current_directory(root);
    process::run_step(&super::wasm_c::configure_step(root, step)?)?;

    let asset_directory = target
        .join(ARTIFACT_INTERNAL_RELEASE_DIRECTORY)
        .join("public/assets");
    let files = collect_tree_files(&asset_directory)?;
    let script = find_generated_asset(&files, "gtl-artifact-dxh", "js")?;
    let wasm = find_generated_asset(&files, "gtl-artifact_bg-dxh", "wasm")?;
    let wasm_name = wasm
        .file_name()
        .and_then(|name| name.to_str())
        .context("generated artifact WASM file name is not UTF-8")?;
    let source = fs::read_to_string(&script)
        .with_context(|| format!("read generated artifact runtime {}", script.display()))?;
    let runtime = embed_artifact_wasm_loader(&source, wasm_name)?;
    let wasm_bytes = fs::read(&wasm)
        .with_context(|| format!("read generated artifact WASM {}", wasm.display()))?;
    ensure!(!wasm_bytes.is_empty(), "generated artifact WASM is empty");
    ensure!(
        u64::try_from(wasm_bytes.len()).unwrap_or(u64::MAX) <= BUNDLE_FILE_BYTES_MAX,
        "generated artifact WASM exceeds {BUNDLE_FILE_BYTES_MAX} bytes"
    );

    replace_artifact_runtime_cache(root, &runtime, &wasm_bytes)
}

fn replace_artifact_runtime_cache(root: &Path, runtime: &str, wasm: &[u8]) -> Result<()> {
    let packed_wasm = pack_artifact_wasm(wasm)?;
    let output_directory = root.join(ARTIFACT_RUNTIME_CACHE_DIRECTORY);
    let output_parent = output_directory
        .parent()
        .context("artifact runtime cache has no parent")?;
    fs::create_dir_all(output_parent)
        .with_context(|| format!("create {}", output_parent.display()))?;
    let staging_directory = tempfile::Builder::new()
        .prefix(".gtl-artifacts-")
        .tempdir_in(output_parent)
        .with_context(|| format!("stage artifact runtime in {}", output_parent.display()))?;
    fs::write(
        staging_directory.path().join("artifact-runtime.js"),
        runtime,
    )
    .context("write staged artifact runtime JavaScript")?;
    fs::write(
        staging_directory
            .path()
            .join("artifact-runtime.wasm.gz.base64"),
        packed_wasm.encoded,
    )
    .context("write staged compressed artifact runtime WASM")?;
    fs::write(
        staging_directory.path().join("artifact-runtime.wasm.bytes"),
        packed_wasm.uncompressed_bytes.to_string(),
    )
    .context("write staged artifact runtime WASM size")?;

    if output_directory.exists() {
        fs::remove_dir_all(&output_directory)
            .with_context(|| format!("replace {}", output_directory.display()))?;
    }
    fs::rename(staging_directory.keep(), &output_directory)
        .with_context(|| format!("publish {}", output_directory.display()))
}

struct PackedArtifactWasm {
    uncompressed_bytes: usize,
    encoded: String,
}

fn pack_artifact_wasm(wasm: &[u8]) -> Result<PackedArtifactWasm> {
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), Compression::best());
    encoder
        .write_all(wasm)
        .context("compress artifact runtime WASM")?;
    let compressed = encoder
        .finish()
        .context("finish artifact runtime WASM compression")?;
    Ok(PackedArtifactWasm {
        uncompressed_bytes: wasm.len(),
        encoded: STANDARD.encode(compressed),
    })
}

fn find_generated_asset(files: &[PathBuf], prefix: &str, extension: &str) -> Result<PathBuf> {
    let matches = files
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix))
                && path
                    .extension()
                    .is_some_and(|candidate| candidate == extension)
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "Dioxus artifact build requires exactly one {prefix}*.{extension} asset"
    );
    Ok(matches[0].clone())
}

fn embed_artifact_wasm_loader(source: &str, wasm_name: &str) -> Result<String> {
    let generated_path = format!("\"/./assets/{wasm_name}\"");
    ensure!(
        source.matches(&generated_path).count() == 1,
        "generated artifact runtime must reference its hashed WASM path exactly once"
    );
    ensure!(
        !source.contains("function gtlArtifactAssetError")
            && !source.contains("__gtlShowArtifactInitializationError"),
        "generated artifact runtime already contains the offline asset loader"
    );
    ensure!(
        !source.to_ascii_lowercase().contains("</script"),
        "generated artifact runtime cannot be embedded safely in HTML"
    );
    ensure!(
        !source.contains("import("),
        "generated artifact runtime contains an external dynamic import"
    );

    let runtime = source.replacen(
        &generated_path,
        &format!(
            "globalThis.__gtlLoadCompressedAsset(\"{ARTIFACT_RUNTIME_ASSET_ID}\",{ARTIFACT_RUNTIME_MAX_BYTES},true)"
        ),
        1,
    );
    let (body, exports) = runtime
        .rsplit_once(";export{")
        .context("generated artifact runtime has no final export block")?;
    let runtime = format!(
        "{ARTIFACT_ASSET_LOADER}\n{body}.catch(globalThis.__gtlShowArtifactInitializationError);export{{{exports}"
    );
    ensure!(
        runtime
            .matches(&format!(
                "globalThis.__gtlLoadCompressedAsset(\"{ARTIFACT_RUNTIME_ASSET_ID}\""
            ))
            .count()
            == 1,
        "embedded artifact runtime must load exactly one compressed WASM asset"
    );
    ensure!(
        !runtime.contains("/./assets/"),
        "embedded artifact runtime retained a sibling asset path"
    );
    Ok(runtime)
}

fn verify_artifact_release_profile(root: &Path) -> Result<()> {
    let manifest_path = root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    verify_artifact_release_profile_text(&manifest)
}

fn verify_artifact_release_profile_text(manifest: &str) -> Result<()> {
    let document = manifest
        .parse::<toml_edit::DocumentMut>()
        .context("parse workspace Cargo.toml")?;
    let profile = document
        .get("profile")
        .and_then(toml_edit::Item::as_table_like)
        .and_then(|profiles| profiles.get(ARTIFACT_PROFILE))
        .and_then(toml_edit::Item::as_table_like)
        .context("workspace Cargo.toml is missing the artifact web release profile")?;
    ensure!(
        profile.get("inherits").and_then(toml_edit::Item::as_str) == Some("release"),
        "artifact web release profile must inherit release"
    );
    ensure!(
        profile.get("opt-level").and_then(toml_edit::Item::as_str) == Some("s"),
        "artifact web release profile must use opt-level s"
    );
    ensure!(
        profile.get("lto").and_then(toml_edit::Item::as_str) == Some("fat"),
        "artifact web release profile must use fat LTO"
    );
    ensure!(
        profile
            .get("codegen-units")
            .and_then(toml_edit::Item::as_integer)
            == Some(1),
        "artifact web release profile must use one codegen unit"
    );
    ensure!(
        profile.get("panic").and_then(toml_edit::Item::as_str) == Some("abort"),
        "artifact web release profile must abort on panic"
    );
    ensure!(
        profile
            .get("incremental")
            .and_then(toml_edit::Item::as_bool)
            == Some(false),
        "artifact web release profile must disable incremental compilation"
    );
    ensure!(
        profile.get("debug").and_then(toml_edit::Item::as_bool) == Some(false),
        "artifact web release profile must disable debug symbols"
    );
    ensure!(
        profile.get("strip").and_then(toml_edit::Item::as_str) == Some("symbols"),
        "artifact web release profile must strip symbols"
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

fn clean_artifact_release_output(target: &Path) -> Result<()> {
    let internal = target.join(ARTIFACT_INTERNAL_RELEASE_DIRECTORY);
    let internal_owner = target.join("dx/gtl-artifact/release");
    ensure!(
        internal.starts_with(&internal_owner) && internal != internal_owner,
        "Dioxus artifact path must remain inside its package release directory"
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
    }

    fn stage_fresh_bundle(root: &Path) {
        let public = root.join(PUBLIC_DIRECTORY);
        fs::create_dir_all(&public).expect("public bundle directory is writable");
        for (name, contents) in [
            ("index.html", "<html></html>"),
            ("assets/app-icon-dxhone.ico", "icon"),
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
            root.path().join("crates/gtl-wire/src/fixture.txt"),
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
    fn release_output_cleanup_is_bounded_to_each_dioxus_application() {
        let root = tempfile::tempdir().expect("temporary repository");
        let target = root.path().join("target");
        let staged = root.path().join(PUBLIC_DIRECTORY).join("old.js");
        let desktop_internal = target
            .join(DESKTOP_INTERNAL_RELEASE_DIRECTORY)
            .join("public/assets/old.js");
        let artifact_internal = target
            .join(ARTIFACT_INTERNAL_RELEASE_DIRECTORY)
            .join("public/assets/old.js");
        let neighbor = target.join("dx/other-package/keep");
        for path in [&staged, &desktop_internal, &artifact_internal, &neighbor] {
            fs::create_dir_all(path.parent().expect("old asset parent"))
                .expect("old asset parent is writable");
            fs::write(path, "old").expect("old asset is writable");
        }

        clean_desktop_release_outputs(root.path(), &target).expect("desktop outputs clean");
        clean_artifact_release_output(&target).expect("artifact outputs clean");

        assert!(!root.path().join(DIST_DIRECTORY).exists());
        assert!(!target.join(DESKTOP_INTERNAL_RELEASE_DIRECTORY).exists());
        assert!(!target.join(ARTIFACT_INTERNAL_RELEASE_DIRECTORY).exists());
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
        assert!(ARTIFACT_BUILD_ARGUMENTS.ends_with(&["--locked"]));
        assert!(
            ARTIFACT_BUILD_ARGUMENTS
                .windows(2)
                .any(|pair| pair == ["--profile", ARTIFACT_PROFILE])
        );
        assert!(!ARTIFACT_BUILD_ARGUMENTS.contains(&"--release"));
        assert!(
            ARTIFACT_BUILD_ARGUMENTS
                .windows(2)
                .any(|pair| pair == ["--bin", "gtl-artifact"])
        );
        assert!(
            ARTIFACT_BUILD_ARGUMENTS
                .windows(2)
                .any(|pair| pair == ["--features", "artifact"])
        );
    }

    #[test]
    fn artifact_wasm_pack_is_deterministic_and_round_trips() {
        let wasm = b"deterministic artifact runtime";
        let first = pack_artifact_wasm(wasm).expect("pack artifact WASM");
        let second = pack_artifact_wasm(wasm).expect("pack artifact WASM again");
        let compressed = STANDARD
            .decode(&first.encoded)
            .expect("decode packed artifact WASM");
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(compressed.as_slice())
            .read_to_end(&mut decoded)
            .expect("decompress packed artifact WASM");

        assert_eq!(first.uncompressed_bytes, wasm.len());
        assert_eq!(first.encoded, second.encoded);
        assert_eq!(decoded, wasm);
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
    fn artifact_runtime_loads_only_the_compressed_wasm_node() {
        let source = r#"const fallback="gtl-artifact_bg.wasm";start({module_or_path:"/./assets/gtl-artifact_bg-dxh123.wasm"});"#;

        let source = format!("{source}export{{start}}");
        let runtime = embed_artifact_wasm_loader(&source, "gtl-artifact_bg-dxh123.wasm")
            .expect("hashed artifact path is replaceable");

        assert!(runtime.contains("module_or_path:globalThis.__gtlLoadCompressedAsset"));
        assert!(runtime.contains("new DecompressionStream(\"gzip\")"));
        assert!(runtime.contains(".catch(globalThis.__gtlShowArtifactInitializationError)"));
        assert!(runtime.contains("fallback=\"gtl-artifact_bg.wasm\""));
        assert!(!runtime.contains("/./assets/"));
        assert!(!runtime.contains("data:application/wasm"));
    }

    #[test]
    fn artifact_runtime_rejects_ambiguous_or_unsafe_scripts() {
        let path = "\"/./assets/gtl-artifact_bg-dxh123.wasm\"";
        let ambiguous = format!("start({path});again({path});export{{start}}");
        assert!(
            embed_artifact_wasm_loader(&ambiguous, "gtl-artifact_bg-dxh123.wasm")
                .expect_err("ambiguous WASM paths must fail")
                .to_string()
                .contains("exactly once")
        );
        assert!(
            embed_artifact_wasm_loader(
                &format!("start({path});</script>;export{{start}}"),
                "gtl-artifact_bg-dxh123.wasm"
            )
            .expect_err("an inline script terminator must fail")
            .to_string()
            .contains("safely")
        );
    }

    #[test]
    fn artifact_release_profile_keeps_every_size_setting_owned() {
        verify_artifact_release_profile_text(include_str!("../../../Cargo.toml"))
            .expect("workspace artifact profile is complete");

        let incomplete = r#"
[profile.release]
strip = "symbols"

[profile.artifact-web-release]
inherits = "release"
opt-level = "s"
"#;
        assert!(
            verify_artifact_release_profile_text(incomplete)
                .expect_err("an incomplete profile must fail")
                .to_string()
                .contains("fat LTO")
        );
    }

    #[test]
    fn development_serve_owns_the_incremental_compile_environment() {
        let root = tempfile::tempdir().expect("temporary repository");
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
        let root = tempfile::tempdir().expect("temporary repository");
        for directory in DEVELOPMENT_RUST_SOURCE_DIRECTORIES {
            fs::create_dir_all(root.path().join(directory))
                .expect("development source directory is writable");
        }
        let before = development_rust_sources(root.path()).expect("initial source inventory");
        let rust_source = root
            .path()
            .join("crates/gtl-web/src/shared/ui/code_text.rs");
        fs::create_dir_all(rust_source.parent().expect("Rust source parent"))
            .expect("Rust source parent is writable");
        fs::write(&rust_source, "pub fn code_text() {}").expect("Rust source fixture is writable");
        fs::write(root.path().join("crates/gtl-web/src/notes.txt"), "not Rust")
            .expect("non-Rust fixture is writable");
        let after = development_rust_sources(root.path()).expect("updated source inventory");

        assert_eq!(new_development_rust_sources(&before, &after), [rust_source]);
    }
}
