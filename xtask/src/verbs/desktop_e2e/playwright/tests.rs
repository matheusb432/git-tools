use std::{cell::Cell, path::Path};

use super::{InstallOutcome, cache_paths, decide_install};

#[test]
fn browser_assets_share_the_repository_playwright_cache_boundary() {
    let cache = cache_paths(Path::new("/repository"));

    assert_eq!(
        cache.driver,
        Path::new("/repository/.cache/playwright-driver")
    );
    assert_eq!(
        cache.browsers,
        Path::new("/repository/.cache/playwright-driver/browsers")
    );
}

#[test]
fn install_stops_on_the_first_success() {
    let calls = Cell::new(0);
    let outcome = decide_install(
        &|| {
            calls.set(calls.get() + 1);
            calls.get() == 2
        },
        4,
    );
    assert_eq!(outcome, InstallOutcome::Installed { attempt: 2 });
}

#[test]
fn install_stops_at_four_attempts() {
    let calls = Cell::new(0);
    assert_eq!(
        decide_install(
            &|| {
                calls.set(calls.get() + 1);
                false
            },
            4
        ),
        InstallOutcome::Failed
    );
    assert_eq!(calls.get(), 4);
}

#[test]
fn preview_readiness_requires_a_complete_event_for_the_expected_url() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("ready.jsonl");
    let address = "127.0.0.1:8091".parse()?;
    std::fs::write(&path, "{\"event\":")?;
    assert!(!super::component_preview_ready(&path, address)?);
    std::fs::write(
        &path,
        "{\"event\":\"ready\",\"url\":\"http://127.0.0.1:8091\"}\n",
    )?;
    assert!(super::component_preview_ready(&path, address)?);
    std::fs::write(
        &path,
        "{\"event\":\"ready\",\"url\":\"http://127.0.0.1:9000\"}\n",
    )?;
    assert!(super::component_preview_ready(&path, address).is_err());
    Ok(())
}
