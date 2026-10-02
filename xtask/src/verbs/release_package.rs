use std::{fmt::Write as _, fs, path::Path};

use anyhow::{Context as _, Result, ensure};
use cargo_metadata::MetadataCommand;
use clap::ValueEnum;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::{cargo_target_directory, repository_root};
use crate::{process, task::Step};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ReleasePlatform {
    Linux,
    Windows,
}

#[derive(Deserialize)]
struct AppConfiguration {
    version: String,
}

pub(crate) fn run(platform: ReleasePlatform) -> Result<()> {
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "Linux and Windows packaging requires an x86_64 Linux host"
    );
    let root = repository_root();
    let target = cargo_target_directory(&root)?;
    let version = release_version(&root)?;
    let (platform_name, suffix, release, extension) = match platform {
        ReleasePlatform::Linux => ("linux", "", target.join("release"), "tar.gz"),
        ReleasePlatform::Windows => (
            "windows",
            ".exe",
            target.join("x86_64-pc-windows-msvc/release"),
            "zip",
        ),
    };
    let name = format!("git-tools-{version}-{platform_name}-x86_64");
    let output = root.join(".artifacts").join(platform_name);
    fs::create_dir_all(&output)?;
    let staging = tempfile::tempdir_in(&output)?;
    let payload = staging.path().join(&name);
    fs::create_dir(&payload)?;
    stage_binaries(&release, &payload, suffix)?;
    fs::copy(root.join("release/INSTALL.md"), payload.join("INSTALL.md"))?;
    fs::copy(root.join("LICENSE"), payload.join("LICENSE"))?;
    match platform {
        ReleasePlatform::Linux => {
            fs::copy(
                root.join("release/linux/install.sh"),
                payload.join("install.sh"),
            )?;
        }
        ReleasePlatform::Windows => {
            for file in ["install.cmd", "uninstall.cmd", "setup.vbs"] {
                fs::copy(root.join("release/windows").join(file), payload.join(file))?;
            }
        }
    }
    let filename = format!("{name}.{extension}");
    let archive = output.join(&filename);
    let candidate = staging.path().join(&filename);
    let arguments = match platform {
        ReleasePlatform::Linux => vec!["-czf".to_owned(), candidate.display().to_string(), name],
        ReleasePlatform::Windows => vec!["-qr".to_owned(), candidate.display().to_string(), name],
    };
    process::run_step(
        &Step::new(
            "archive-release",
            match platform {
                ReleasePlatform::Linux => "tar",
                ReleasePlatform::Windows => "zip",
            },
            arguments,
        )
        .with_current_directory(staging.path()),
    )?;
    fs::rename(candidate, &archive)?;
    let digest = Sha256::digest(fs::read(&archive)?);
    let mut checksum = String::new();
    for byte in digest {
        write!(checksum, "{byte:02x}")?;
    }
    writeln!(checksum, "  {filename}")?;
    fs::write(output.join(format!("{filename}.sha256")), checksum)?;
    println!("Release archive: {}", archive.display());
    package_installer(platform, &root, &target, &release, &output, &version)?;
    Ok(())
}

fn package_installer(
    platform: ReleasePlatform,
    root: &Path,
    target: &Path,
    release: &Path,
    output: &Path,
    version: &str,
) -> Result<()> {
    let (triple, platform_name, suffix, bundle, extension) = match platform {
        ReleasePlatform::Linux => ("x86_64-unknown-linux-gnu", "linux", "", "deb", "deb"),
        ReleasePlatform::Windows => ("x86_64-pc-windows-msvc", "windows", ".exe", "nsis", "exe"),
    };
    let sidecars = root.join("crates/gtl-desktop/binaries");
    fs::create_dir_all(&sidecars)?;
    for binary in ["git-tools", "gtl", "gtl-server"] {
        let source = if binary == "gtl" { "git-tools" } else { binary };
        fs::copy(
            release.join(format!("{source}{suffix}")),
            sidecars.join(format!("{binary}-{triple}{suffix}")),
        )?;
    }
    let staging = tempfile::tempdir_in(target)?;
    let bundle_target = staging.path().join("target");
    let bundle_release = bundle_target.join(triple).join("release");
    fs::create_dir_all(&bundle_release)?;
    fs::copy(
        release.join(format!("gtl-viewer{suffix}")),
        bundle_release.join(format!("gtl-viewer{suffix}")),
    )?;
    process::run_step(
        &Step::new(
            "bundle-release-installer",
            "cargo",
            [
                "tauri",
                "bundle",
                "--ci",
                "--target",
                triple,
                "--features",
                "custom-protocol",
                "--config",
                "tauri.production.conf.json",
                "--config",
                &format!("tauri.{platform_name}.bundle.conf.json"),
            ],
        )
        .with_current_directory(root.join("crates/gtl-desktop"))
        .with_environment("CARGO_TARGET_DIR", bundle_target.display().to_string()),
    )?;
    let candidates = fs::read_dir(bundle_release.join("bundle").join(bundle))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let installers = candidates
        .iter()
        .filter(|path| path.extension().is_some_and(|value| value == extension))
        .collect::<Vec<_>>();
    ensure!(
        installers.len() == 1,
        "expected one {platform_name} installer"
    );
    let filename = match platform {
        ReleasePlatform::Linux => format!("git-tools-{version}-linux-x86_64.deb"),
        ReleasePlatform::Windows => format!("git-tools-{version}-windows-x86_64-setup.exe"),
    };
    let installer = output.join(&filename);
    fs::copy(installers[0], &installer)?;
    let digest = Sha256::digest(fs::read(&installer)?);
    let mut checksum = String::new();
    for byte in digest {
        write!(checksum, "{byte:02x}")?;
    }
    writeln!(checksum, "  {filename}")?;
    fs::write(output.join(format!("{filename}.sha256")), checksum)?;
    println!("Release installer: {}", installer.display());
    Ok(())
}

pub(super) fn release_version(root: &Path) -> Result<String> {
    let configuration: AppConfiguration =
        serde_json::from_slice(&fs::read(root.join("crates/gtl-desktop/tauri.conf.json"))?)?;
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .current_dir(root)
        .no_deps()
        .other_options(vec!["--locked".to_owned()])
        .exec()?;
    for name in ["gtl-cli", "gtl-server", "gtl-desktop"] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name.as_str() == name)
            .with_context(|| format!("missing release package {name}"))?;
        ensure!(
            package.version.to_string() == configuration.version,
            "{name} version {} differs from desktop version {}",
            package.version,
            configuration.version
        );
    }
    Ok(configuration.version)
}

fn stage_binaries(release: &Path, payload: &Path, suffix: &str) -> Result<()> {
    for binary in ["git-tools", "gtl-server", "gtl-viewer"] {
        let name = format!("{binary}{suffix}");
        let source = release.join(&name);
        ensure!(
            fs::metadata(&source).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0),
            "missing or empty release executable {}; build all components first",
            source.display()
        );
        fs::copy(&source, payload.join(name))?;
    }
    fs::copy(
        payload.join(format!("git-tools{suffix}")),
        payload.join(format!("gtl{suffix}")),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_release_includes_an_independent_cli_alias() {
        for suffix in ["", ".exe"] {
            let temporary = tempfile::tempdir().unwrap();
            let release = temporary.path().join("release");
            let payload = temporary.path().join("payload");
            fs::create_dir(&release).unwrap();
            fs::create_dir(&payload).unwrap();
            for binary in ["git-tools", "gtl-server", "gtl-viewer"] {
                fs::write(release.join(format!("{binary}{suffix}")), binary).unwrap();
            }

            stage_binaries(&release, &payload, suffix).unwrap();
            fs::remove_dir_all(release).unwrap();

            for binary in ["git-tools", "gtl-server", "gtl-viewer"] {
                assert_eq!(
                    fs::read(payload.join(format!("{binary}{suffix}"))).unwrap(),
                    binary.as_bytes()
                );
            }
            assert_eq!(
                fs::read(payload.join(format!("gtl{suffix}"))).unwrap(),
                b"git-tools"
            );
        }
    }

    #[test]
    fn release_refuses_an_absent_or_empty_viewer() {
        let temporary = tempfile::tempdir().unwrap();
        let release = temporary.path().join("release");
        let payload = temporary.path().join("payload");
        fs::create_dir(&release).unwrap();
        fs::create_dir(&payload).unwrap();
        for binary in ["git-tools", "gtl-server"] {
            fs::write(release.join(binary), binary).unwrap();
        }
        for viewer in [None, Some(b"")] {
            if let Some(bytes) = viewer {
                fs::write(release.join("gtl-viewer"), bytes).unwrap();
            }
            let error = stage_binaries(&release, &payload, "").unwrap_err();
            assert!(error.to_string().contains("gtl-viewer"), "{error}");
        }
    }
}
