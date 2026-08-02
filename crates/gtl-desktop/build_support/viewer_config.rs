use std::{
    fs,
    path::{Path, PathBuf},
};

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
}

pub(crate) fn load(manifest_dir: &Path) -> ResolvedViewerConfig {
    let config_path = manifest_dir.join("viewer.toml");
    let raw = fs::read_to_string(&config_path).expect("viewer.toml must be readable");
    let config: ViewerConfig = toml::from_str(&raw).expect("viewer.toml must match its schema");
    let app_url = url::Url::parse(&format!(
        "{}://{}/",
        config.protocol.scheme, config.protocol.host
    ))
    .expect("viewer protocol scheme and host must form a valid URL");
    assert_eq!(
        app_url.path(),
        "/",
        "viewer protocol URL must not contain a path"
    );
    assert!(
        app_url.query().is_none(),
        "viewer protocol URL must not contain a query"
    );
    assert!(
        config.assets.htmx_path.is_relative(),
        "htmx asset path must be relative to the desktop crate"
    );
    let htmx_path = manifest_dir
        .join(config.assets.htmx_path)
        .canonicalize()
        .expect("configured htmx asset must exist");
    let manifest_dir = manifest_dir
        .canonicalize()
        .expect("desktop manifest directory must exist");
    assert!(
        htmx_path.starts_with(&manifest_dir),
        "htmx asset must remain inside the desktop crate"
    );
    assert!(
        htmx_path.is_file(),
        "configured htmx asset must be a regular file"
    );

    ResolvedViewerConfig {
        config_path,
        htmx_path,
        app_url,
    }
}
