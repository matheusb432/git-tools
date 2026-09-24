use std::{future::Future, time::Duration};

use anyhow::{Context, Result, ensure};
use thirtyfour::error::{WebDriverError, WebDriverErrorInner};
use tokio::time::{Instant, sleep, timeout, timeout_at};

pub const ASSERTION_TIMEOUT: Duration = Duration::from_secs(30);
pub const WEBDRIVER_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub async fn within<T, F>(description: &str, duration: Duration, operation: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    timeout(duration, operation)
        .await
        .with_context(|| format!("timed out after {duration:?} {description}"))?
}

/// Polls `probe` until it returns a value.
///
/// A probe that reads an element the viewer is re-rendering, or one that is not mounted yet,
/// is retried instead of failing the wait.
pub async fn until<T, F, Fut>(description: &str, duration: Duration, mut probe: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
{
    let deadline = Instant::now() + duration;
    loop {
        let probe_result = timeout_at(deadline, probe())
            .await
            .with_context(|| format!("timed out after {duration:?} waiting for {description}"))?;
        match probe_result {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) if is_transient_dom_error(&error) => {}
            Err(error) => return Err(error),
        }

        let now = Instant::now();
        ensure!(
            now < deadline,
            "timed out after {duration:?} waiting for {description}"
        );
        sleep(POLL_INTERVAL.min(deadline - now)).await;
    }
}

fn is_transient_dom_error(error: &anyhow::Error) -> bool {
    matches!(
        error
            .downcast_ref::<WebDriverError>()
            .map(WebDriverError::as_inner),
        Some(WebDriverErrorInner::StaleElementReference(_) | WebDriverErrorInner::NoSuchElement(_))
    )
}
