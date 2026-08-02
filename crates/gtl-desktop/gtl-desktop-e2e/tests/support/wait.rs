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

#[cfg(test)]
mod tests {
    use std::{future, time::Duration};

    use super::{until, within};

    #[tokio::test]
    async fn until_reports_the_owned_description_at_the_deadline() {
        let error = until("viewer ready layout", Duration::from_millis(1), || async {
            Ok::<_, anyhow::Error>(None::<()>)
        })
        .await
        .unwrap_err();
        assert!(error.to_string().contains("viewer ready layout"));
    }

    #[tokio::test]
    async fn until_bounds_a_probe_that_never_finishes() {
        let error = until("stalled WebDriver probe", Duration::from_millis(1), || {
            future::pending::<anyhow::Result<Option<()>>>()
        })
        .await
        .unwrap_err();

        assert!(error.to_string().contains("stalled WebDriver probe"));
    }

    #[tokio::test]
    async fn within_reports_the_owned_description_at_the_deadline() {
        let error = within(
            "capture viewer evidence",
            Duration::from_millis(1),
            future::pending::<anyhow::Result<()>>(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("capture viewer evidence"));
    }
}
