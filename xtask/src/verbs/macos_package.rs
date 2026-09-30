//! Native macOS packaging and installed-artifact smoke checks.

use std::{fmt::Write as _, fs, path::Path, process::Command, thread, time::Duration};

use anyhow::{Context as _, Result, bail, ensure};
use sha2::{Digest as _, Sha256};

use super::{build, cargo_target_directory, release_package::release_version, repository_root};
use crate::{cli::BuildTarget, process, task::Step};

const APP_NAME: &str = "gtl-viewer.app";
const INSTALLED_APP: &str = "/Applications/gtl-viewer.app";
const INSTALLED_CLI: &str = "/usr/local/bin/git-tools";
const LAUNCH_AGENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>gtl-server</string>
  <key>ProgramArguments</key><array><string>/Applications/gtl-viewer.app/Contents/MacOS/gtl-server</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ThrottleInterval</key><integer>2</integer>
  <key>EnvironmentVariables</key><dict>
    <key>PATH</key><string>/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
</dict></plist>
"#;
const COMPONENTS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array><dict>
  <key>RootRelativeBundlePath</key><string>Applications/gtl-viewer.app</string>
  <key>BundleIsRelocatable</key><false/>
  <key>BundleHasStrictIdentifier</key><true/>
  <key>BundleIsVersionChecked</key><false/>
  <key>BundleOverwriteAction</key><string>upgrade</string>
</dict></array></plist>
"#;
// Installer runs as root. Load the agent only into an existing graphical user session;
// /Library/LaunchAgents also registers it for subsequent logins and offline installs.
const POSTINSTALL: &str = r#"#!/bin/sh
set -eu
if [ "$3" != / ]; then
    exit 0
fi
console_uid=$(/usr/bin/stat -f %u /dev/console)
if [ "$console_uid" -ne 0 ] && /bin/launchctl print "gui/$console_uid" >/dev/null 2>&1; then
    /bin/launchctl bootout "gui/$console_uid/gtl-server" >/dev/null 2>&1 || true
    /bin/launchctl bootstrap "gui/$console_uid" /Library/LaunchAgents/gtl-server.plist
fi
"#;

pub(crate) fn run() -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "macOS packaging requires a macOS host"
    );
    let architecture = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        architecture => bail!("unsupported macOS architecture: {architecture}"),
    };
    build::run(BuildTarget::Both)?;
    let root = repository_root();
    let target = cargo_target_directory(&root)?;
    let release = target.join("release");
    let host =
        process::capture_bytes("read Rust host target", "rustc", &["--print", "host-tuple"])?;
    let host = std::str::from_utf8(&host)?.trim();
    stage_sidecars(&root, &release, host)?;
    let staging = tempfile::tempdir_in(&target).context("create macOS package staging")?;
    let bundle_target = staging.path().join("target");
    let bundle_release = bundle_target.join(host).join("release");
    // An explicit target avoids inheriting the prebuilt Tauri CLI's architecture.
    // `tauri bundle --target` expects Cargo's target-specific directory layout.
    fs::create_dir_all(&bundle_release)?;
    fs::copy(
        release.join("gtl-viewer"),
        bundle_release.join("gtl-viewer"),
    )
    .context("stage the native viewer for target-specific bundling")?;
    process::run_step(
        &Step::new(
            "bundle-macos-app",
            "cargo",
            [
                "tauri",
                "bundle",
                "--ci",
                "--bundles",
                "app",
                "--target",
                host,
                "--features",
                "custom-protocol",
                "--config",
                "tauri.production.conf.json",
                "--config",
                "tauri.macos.bundle.conf.json",
            ],
        )
        .with_current_directory(root.join("crates/gtl-desktop"))
        .with_environment("CARGO_TARGET_DIR", bundle_target.display().to_string()),
    )?;
    let app = bundle_release.join("bundle/macos").join(APP_NAME);
    verify_app(&app)?;

    let payload = staging.path().join("root");
    let scripts = staging.path().join("scripts");
    stage_payload(&app, &payload, &scripts)?;
    let components = staging.path().join("components.plist");
    fs::write(&components, COMPONENTS).context("write package component policy")?;
    let version = release_version(&root)?;
    let output = root.join(".artifacts/macos");
    fs::create_dir_all(&output)?;
    let filename = format!("git-tools-{version}-macos-{architecture}.pkg");
    let package = output.join(&filename);
    process::run_step(&Step::new(
        "build-macos-installer",
        "pkgbuild",
        [
            "--root".to_owned(),
            payload.display().to_string(),
            "--scripts".to_owned(),
            scripts.display().to_string(),
            "--component-plist".to_owned(),
            components.display().to_string(),
            "--identifier".to_owned(),
            "dev.gittools.installer".to_owned(),
            "--version".to_owned(),
            version,
            "--install-location".to_owned(),
            "/".to_owned(),
            "--ownership".to_owned(),
            "recommended".to_owned(),
            package.display().to_string(),
        ],
    ))?;
    let digest = Sha256::digest(fs::read(&package).context("read completed macOS installer")?);
    let mut checksum = String::new();
    for byte in digest {
        write!(checksum, "{byte:02x}")?;
    }
    writeln!(checksum, "  {filename}")?;
    fs::write(output.join(format!("{filename}.sha256")), checksum)?;
    println!("macOS installer: {}", package.display());
    Ok(())
}

fn stage_sidecars(root: &Path, release: &Path, host: &str) -> Result<()> {
    let sidecars = root.join("crates/gtl-desktop/binaries");
    fs::create_dir_all(&sidecars)?;
    for binary in ["git-tools", "gtl-server"] {
        fs::copy(
            release.join(binary),
            sidecars.join(format!("{binary}-{host}")),
        )
        .with_context(|| format!("stage {binary} for the macOS bundle"))?;
    }
    Ok(())
}

fn verify_app(app: &Path) -> Result<()> {
    for binary in ["git-tools", "gtl-server", "gtl-viewer"] {
        let path = app.join("Contents/MacOS").join(binary);
        ensure!(path.is_file(), "macOS app is missing {}", path.display());
    }
    process::run_step(&Step::new(
        "verify-macos-signature",
        "codesign",
        [
            "--verify".to_owned(),
            "--deep".to_owned(),
            "--strict".to_owned(),
            app.display().to_string(),
        ],
    ))
}

fn stage_payload(app: &Path, payload: &Path, scripts: &Path) -> Result<()> {
    let destination = payload.join("Applications").join(APP_NAME);
    fs::create_dir_all(destination.parent().context("app install parent")?)?;
    process::run_step(&Step::new(
        "stage-macos-app",
        "ditto",
        [app.display().to_string(), destination.display().to_string()],
    ))?;
    let bin = payload.join("usr/local/bin");
    fs::create_dir_all(&bin)?;
    for (name, executable) in [
        ("git-tools", "git-tools"),
        ("gtl", "git-tools"),
        ("gtl-server", "gtl-server"),
        ("gtl-viewer", "gtl-viewer"),
    ] {
        process::run_step(&Step::new(
            "stage-macos-command",
            "ln",
            [
                "-s".to_owned(),
                format!("{INSTALLED_APP}/Contents/MacOS/{executable}"),
                bin.join(name).display().to_string(),
            ],
        ))?;
    }
    let agents = payload.join("Library/LaunchAgents");
    fs::create_dir_all(&agents)?;
    fs::write(agents.join("gtl-server.plist"), LAUNCH_AGENT)?;
    fs::create_dir_all(scripts)?;
    let postinstall = scripts.join("postinstall");
    fs::write(&postinstall, POSTINSTALL)?;
    process::run_step(&Step::new(
        "make-macos-installer-script-executable",
        "chmod",
        ["755".to_owned(), postinstall.display().to_string()],
    ))
}

pub(crate) fn smoke() -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "macOS smoke checks require a macOS host"
    );
    verify_app(Path::new(INSTALLED_APP))?;
    wait_for_server()?;
    process::run_step(&Step::new(
        "installed-project-list",
        INSTALLED_CLI,
        ["project", "ls"],
    ))?;
    let user_id = process::capture_bytes("read current user ID", "id", &["-u"])?;
    let user_id = std::str::from_utf8(&user_id)?.trim();
    process::run_step(&Step::new(
        "restart-installed-server",
        "launchctl",
        [
            "kickstart".to_owned(),
            "-k".to_owned(),
            format!("gui/{user_id}/gtl-server"),
        ],
    ))?;
    wait_for_server()?;
    let repository = smoke_repository()?;
    process::run_step(
        &Step::new(
            "installed-raw-diff",
            INSTALLED_CLI,
            ["diff", "--raw", "--last", "1"],
        )
        .with_current_directory(repository.path()),
    )?;
    process::run_step(
        &Step::new(
            "installed-desktop-diff",
            INSTALLED_CLI,
            ["diff", "--last", "1"],
        )
        .with_current_directory(repository.path()),
    )?;
    thread::sleep(Duration::from_secs(5));
    process::run_step(&Step::new(
        "check-viewer-process",
        "pgrep",
        ["-x", "gtl-viewer"],
    ))?;
    let output = repository_root().join(".artifacts/macos");
    fs::create_dir_all(&output)?;
    process::run_step(&Step::new(
        "capture-macos-viewer",
        "screencapture",
        [
            "-x".to_owned(),
            output.join("viewer.png").display().to_string(),
        ],
    ))?;
    println!("Installed CLI, gRPC service, raw diff, and desktop launch checks passed.");
    Ok(())
}

fn wait_for_server() -> Result<()> {
    let mut failure = String::new();
    for _ in 0..30 {
        let output = Command::new(INSTALLED_CLI)
            .args(["server", "status"])
            .output()
            .context("check installed server readiness")?;
        if output.status.success() {
            return Ok(());
        }
        failure = String::from_utf8_lossy(&output.stderr).into_owned();
        thread::sleep(Duration::from_millis(200));
    }
    bail!("installed server did not become ready: {failure}")
}

fn smoke_repository() -> Result<tempfile::TempDir> {
    let repository = tempfile::tempdir().context("create macOS smoke repository")?;
    let path = repository.path();
    for arguments in [
        vec!["init", "--initial-branch=main"],
        vec!["config", "user.name", "Git Tools CI"],
        vec!["config", "user.email", "git-tools-ci@example.invalid"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        process::run_step(
            &Step::new("prepare-smoke-repository", "git", arguments).with_current_directory(path),
        )?;
    }
    for text in [
        "fn main() {}\n",
        "fn main() { println!(\"macOS package smoke\"); }\n",
    ] {
        fs::write(path.join("main.rs"), text)?;
        process::run_step(
            &Step::new("stage-smoke-commit", "git", ["add", "main.rs"])
                .with_current_directory(path),
        )?;
        process::run_step(
            &Step::new(
                "create-smoke-commit",
                "git",
                ["commit", "-m", "smoke fixture"],
            )
            .with_current_directory(path),
        )?;
    }
    Ok(repository)
}
