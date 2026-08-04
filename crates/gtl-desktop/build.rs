use std::{env, path::PathBuf};

#[path = "build_support/viewer_config.rs"]
mod viewer_config;

fn emit_viewer_config(config: &viewer_config::ResolvedViewerConfig) {
    println!("cargo:rerun-if-changed={}", config.config_path.display());
    println!("cargo:rerun-if-changed={}", config.htmx_path.display());
    println!(
        "cargo:rustc-env=GTL_VIEWER_PROTOCOL_SCHEME={}",
        config.app_url.scheme()
    );
    println!("cargo:rustc-env=GTL_VIEWER_APP_HOST={}", config.app_host);
    println!("cargo:rustc-env=GTL_VIEWER_APP_URL={}", config.app_url);
    println!(
        "cargo:rustc-env=GTL_VIEWER_HTMX_PATH={}",
        config.htmx_path.display()
    );
}

fn main() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| anyhow::anyhow!("Cargo did not set CARGO_MANIFEST_DIR"))?,
    );
    let config = viewer_config::load(&manifest_dir)?;
    emit_viewer_config(&config);
    tauri_build::build();
    Ok(())
}
