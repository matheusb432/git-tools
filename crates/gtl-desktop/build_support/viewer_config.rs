use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, ensure};
use serde::Deserialize;

#[derive(Deserialize)]
struct ViewerConfig {
    protocol: ProtocolConfig,
    assets: AssetConfig,
}

#[derive(Deserialize)]
struct ProtocolConfig {
    scheme: String,
    host: String,
}

#[derive(Deserialize)]
struct AssetConfig {
    htmx_path: PathBuf,
}

pub(crate) struct ResolvedViewerConfig {
    pub(crate) config_path: PathBuf,
    pub(crate) htmx_path: PathBuf,
    pub(crate) app_url: url::Url,
    pub(crate) app_host: String,
}

pub(crate) fn load(manifest_dir: &Path) -> anyhow::Result<ResolvedViewerConfig> {
    let config_path = manifest_dir.join("viewer.toml");
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let config: ViewerConfig =
        toml::from_str(&raw).with_context(|| format!("parse {}", config_path.display()))?;
    let app_url = url::Url::parse(&format!(
        "{}://{}/",
        config.protocol.scheme, config.protocol.host
    ))
    .context("viewer protocol scheme and host must form a valid URL")?;
    ensure!(
        app_url.path() == "/",
        "viewer protocol URL must not contain a path"
    );
    ensure!(
        app_url.query().is_none(),
        "viewer protocol URL must not contain a query"
    );
    ensure!(
        config.assets.htmx_path.is_relative(),
        "htmx asset path must be relative to the desktop crate"
    );
    let htmx_path = manifest_dir
        .join(config.assets.htmx_path)
        .canonicalize()
        .context("configured htmx asset must exist")?;
    let manifest_dir = manifest_dir
        .canonicalize()
        .context("desktop manifest directory must exist")?;
    ensure!(
        htmx_path.starts_with(&manifest_dir),
        "htmx asset must remain inside the desktop crate"
    );
    ensure!(
        htmx_path.is_file(),
        "configured htmx asset must be a regular file"
    );

    Ok(ResolvedViewerConfig {
        config_path,
        htmx_path,
        app_url,
        app_host: config.protocol.host,
    })
}
