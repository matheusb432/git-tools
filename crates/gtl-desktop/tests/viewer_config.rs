#![cfg(test)]

use std::{fs, path::Path};

use tempfile::TempDir;

#[path = "../build_support/viewer_config.rs"]
mod viewer_config;

fn write_config(manifest_dir: &Path, scheme: &str, asset_path: &str) {
    fs::write(
        manifest_dir.join("viewer.toml"),
        format!(
            "[protocol]\nscheme = \"{scheme}\"\nhost = \"app\"\n\n[assets]\nhtmx_path = \"{asset_path}\"\n"
        ),
    )
    .expect("test viewer config is writable");
}

#[test]
fn resolves_valid_origin_and_asset_path() {
    let manifest_dir = TempDir::new().expect("temporary desktop crate is available");
    let asset_dir = manifest_dir.path().join("src/embedded");
    fs::create_dir_all(&asset_dir).expect("test asset directory is creatable");
    let asset_path = asset_dir.join("htmx.js");
    fs::write(&asset_path, "// htmx").expect("test htmx asset is writable");
    write_config(manifest_dir.path(), "gtl", "src/embedded/htmx.js");

    let resolved = viewer_config::load(manifest_dir.path()).expect("valid config should resolve");

    assert_eq!(resolved.app_url.as_str(), "gtl://app/");
    assert_eq!(resolved.app_host, "app");
    assert_eq!(
        resolved.config_path,
        manifest_dir.path().join("viewer.toml")
    );
    assert_eq!(
        resolved.htmx_path,
        asset_path
            .canonicalize()
            .expect("test htmx asset is canonicalizable")
    );
}

#[test]
fn rejects_an_invalid_origin() {
    let manifest_dir = TempDir::new().expect("temporary desktop crate is available");
    let asset_dir = manifest_dir.path().join("src/embedded");
    fs::create_dir_all(&asset_dir).expect("test asset directory is creatable");
    fs::write(asset_dir.join("htmx.js"), "// htmx").expect("test htmx asset is writable");
    write_config(manifest_dir.path(), "1invalid", "src/embedded/htmx.js");

    let Err(error) = viewer_config::load(manifest_dir.path()) else {
        panic!("invalid origin should fail");
    };

    assert!(
        error
            .to_string()
            .contains("viewer protocol scheme and host must form a valid URL")
    );
}

#[test]
fn rejects_a_directory_as_the_htmx_asset() {
    let manifest_dir = TempDir::new().expect("temporary desktop crate is available");
    fs::create_dir_all(manifest_dir.path().join("src/embedded"))
        .expect("test asset directory is creatable");
    write_config(manifest_dir.path(), "gtl", "src/embedded");

    let Err(error) = viewer_config::load(manifest_dir.path()) else {
        panic!("directory asset should fail");
    };

    assert!(
        error
            .to_string()
            .contains("configured htmx asset must be a regular file")
    );
}
