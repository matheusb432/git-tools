use anyhow::{Context as _, Result};
use assert_cmd::Command;
use predicates::str::contains;

mod common;

#[test]
fn diff_set_theme_updates_the_user_settings_document() -> Result<()> {
    let directory = tempfile::tempdir().context("temp directory")?;
    let config = directory.path().join("nested").join("config.toml");
    let config_parent = config.parent().context("config parent")?;
    std::fs::create_dir_all(config_parent).context("create config parent")?;
    std::fs::write(&config, "layout = \"split\"\n").context("seed config")?;
    let _server = common::ServerHarness::start(Some(&config))?;

    Command::new(env!("CARGO_BIN_EXE_git-tools"))
        .args(["diff", "--set-theme", "hearth"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .success()
        .stdout(contains("hearth"))
        .stdout(contains(config.display().to_string()));

    let raw = std::fs::read_to_string(config).context("read updated config")?;
    let document = toml::from_str::<toml::Value>(&raw).context("parse settings TOML")?;
    assert_eq!(document["theme"].as_str(), Some("hearth"));
    assert_eq!(document["layout"].as_str(), Some("split"));
    Ok(())
}
