use std::{future::Future, time::Duration};

use anyhow::{Context, Result, ensure};
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

pub async fn until<T, F, Fut>(description: &str, duration: Duration, mut probe: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
{
    let deadline = Instant::now() + duration;
    loop {
        let probe_result = timeout_at(deadline, probe())
            .await
            .with_context(|| format!("timed out after {duration:?} waiting for {description}"))??;
        if let Some(value) = probe_result {
            return Ok(value);
        }

        let now = Instant::now();
        ensure!(
            now < deadline,
            "timed out after {duration:?} waiting for {description}"
        );
        sleep(POLL_INTERVAL.min(deadline - now)).await;
    }
}
