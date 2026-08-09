use std::{future::Future, panic::AssertUnwindSafe, path::PathBuf, pin::Pin};

use anyhow::{Context, Result};
use futures_util::FutureExt;

pub mod evidence;
pub mod fixture;
pub mod selectors;
pub mod session;
pub mod wait;

mod journey;

pub use journey::{
    assert_chunked_live_view, assert_configured_editor_launch, assert_default_navigation_reachable,
    assert_durable_empty_state, assert_first_paint, assert_forwarded_live_view,
    assert_mobile_navigation, assert_overlapping_live_updates, assert_restarted_live_view,
    delete_and_assert_empty_state, delete_temporary_live_views, refresh_and_assert_alpha_v2,
    refresh_and_assert_unavailable, select_commit_patch_and_restore_range, select_split_layout,
};

pub async fn run_test<F>(name: &'static str, body: F) -> Result<()>
where
    F: for<'session> FnOnce(
        &'session mut session::TestSession,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'session>>,
{
    let mut session = session::TestSession::start(name).await?;
    let body_result = AssertUnwindSafe(body(&mut session)).catch_unwind().await;
    let passed = matches!(&body_result, Ok(Ok(())));
    let evidence_result = match session.driver_if_active() {
        Some(driver) => evidence::capture(driver, name, passed).await,
        None => Ok(None),
    };
    let cleanup_result = session.finish().await;

    match body_result {
        Ok(Ok(())) => {
            attach_secondary_error(evidence_result.map(|_| ()), cleanup_result, "test cleanup")
        }
        Ok(Err(body_error)) => {
            let result = attach_evidence(Err(body_error), evidence_result);
            attach_secondary_error(result, cleanup_result, "test cleanup")
        }
        Err(panic) => {
            match evidence_result {
                Ok(Some(path)) => eprintln!("evidence saved after panic: {}", path.display()),
                Ok(None) => {}
                Err(error) => eprintln!("evidence capture failed after panic: {error:#}"),
            }
            if let Err(error) = cleanup_result {
                eprintln!("test cleanup failed after panic: {error:#}");
            }
            std::panic::resume_unwind(panic);
        }
    }
}

fn attach_evidence(primary: Result<()>, evidence: Result<Option<PathBuf>>) -> Result<()> {
    match (primary, evidence) {
        (Ok(()), Ok(_)) => Ok(()),
        (Ok(()), Err(error)) => Err(error),
        (Err(primary_error), Ok(Some(path))) => {
            Err(primary_error).context(format!("evidence saved to {}", path.display()))
        }
        (Err(primary_error), Ok(None)) => Err(primary_error),
        (Err(primary_error), Err(error)) => {
            Err(primary_error).context(format!("evidence capture also failed: {error:#}"))
        }
    }
}

fn attach_secondary_error(primary: Result<()>, secondary: Result<()>, label: &str) -> Result<()> {
    match (primary, secondary) {
        (Ok(()), result) => result,
        (Err(primary_error), Ok(())) => Err(primary_error),
        (Err(primary_error), Err(secondary_error)) => {
            Err(primary_error).context(format!("{label} also failed: {secondary_error:#}"))
        }
    }
}
