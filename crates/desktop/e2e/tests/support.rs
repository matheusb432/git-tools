#![allow(dead_code)]

use std::{future::Future, panic::AssertUnwindSafe, pin::Pin};

use anyhow::{Context, Result};
use futures_util::FutureExt;

pub mod evidence;
pub mod fixture;
pub mod selectors;
pub mod session;
pub mod wait;

mod journey;
mod refresh_delivery;

pub use journey::{
    assert_configured_editor_launch, assert_first_paint, assert_forwarded_live_view,
    assert_mobile_navigation, delete_and_restore_empty_state, refresh_and_assert_alpha_v2,
    select_and_restore_split_layout,
};

pub async fn run_test<F>(name: &'static str, body: F) -> Result<()>
where
    F: for<'session> FnOnce(
        &'session session::TestSession,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'session>>,
{
    let session = session::TestSession::start().await?;
    let body_result = AssertUnwindSafe(body(&session)).catch_unwind().await;
    let outcome = if matches!(&body_result, Ok(Ok(()))) {
        "success"
    } else {
        "fail"
    };
    let evidence_result = evidence::capture(session.driver(), name, outcome).await;
    let cleanup_result = session.finish().await;

    match body_result {
        Ok(Ok(())) => attach_secondary_error(evidence_result, cleanup_result, "test cleanup"),
        Ok(Err(body_error)) => {
            let result =
                attach_secondary_error(Err(body_error), evidence_result, "evidence capture");
            attach_secondary_error(result, cleanup_result, "test cleanup")
        }
        Err(panic) => {
            if let Err(error) = evidence_result {
                eprintln!("evidence capture failed after panic: {error:#}");
            }
            if let Err(error) = cleanup_result {
                eprintln!("test cleanup failed after panic: {error:#}");
            }
            std::panic::resume_unwind(panic);
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

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::attach_secondary_error;

    #[test]
    fn primary_assertion_error_survives_screenshot_and_cleanup_failures() {
        let result = attach_secondary_error(
            Err(anyhow!("expected live view")),
            Err(anyhow!("screenshot unavailable")),
            "evidence capture",
        );
        let error =
            attach_secondary_error(result, Err(anyhow!("cleanup unavailable")), "test cleanup")
                .unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains("expected live view"));
        assert!(message.contains("screenshot unavailable"));
        assert!(message.contains("cleanup unavailable"));
    }
}
