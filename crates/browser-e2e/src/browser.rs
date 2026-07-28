use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result, bail, ensure};
use playwright_rs::{
    LaunchOptions, Playwright,
    protocol::{Browser, BrowserContext, Page},
};

pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Session {
    playwright: Playwright,
    browser: Browser,
    pub context: BrowserContext,
    pub page: Page,
    blocked_urls: Arc<Mutex<Vec<String>>>,
}

pub async fn with_timeout<T>(
    label: &str,
    timeout: Duration,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(timeout, operation)
        .await
        .with_context(|| format!("{label} timed out after {}ms", timeout.as_millis()))?
}

pub async fn operation<T>(label: &str, operation: impl Future<Output = Result<T>>) -> Result<T> {
    with_timeout(label, OPERATION_TIMEOUT, operation).await
}

pub async fn open() -> Result<Session> {
    let playwright = operation("launch Playwright driver", async {
        Playwright::launch()
            .await
            .context("launch Playwright driver")
    })
    .await?;
    let browser = match operation("launch Chromium", async {
        playwright
            .chromium()
            .launch_with_options(LaunchOptions::new().args(vec![
                "--disable-background-networking".to_owned(),
                "--disable-component-update".to_owned(),
                "--disable-default-apps".to_owned(),
                "--disable-sync".to_owned(),
                "--metrics-recording-only".to_owned(),
            ]))
            .await
            .context("launch Chromium")
    })
    .await
    {
        Ok(browser) => browser,
        Err(error) => return fail_after_cleanup(error, None, None, None, &playwright).await,
    };
    let context = match operation("create isolated Chromium context", async {
        browser
            .new_context()
            .await
            .context("create isolated Chromium context")
    })
    .await
    {
        Ok(context) => context,
        Err(error) => {
            return fail_after_cleanup(error, None, None, Some(&browser), &playwright).await;
        }
    };
    if let Err(error) = operation("set Chromium action deadline", async {
        context
            .set_default_timeout(OPERATION_TIMEOUT.as_secs_f64() * 1_000.0)
            .await;
        Ok(())
    })
    .await
    {
        return fail_after_cleanup(error, None, Some(&context), Some(&browser), &playwright).await;
    }
    if let Err(error) = operation("set Chromium navigation deadline", async {
        context
            .set_default_navigation_timeout(OPERATION_TIMEOUT.as_secs_f64() * 1_000.0)
            .await;
        Ok(())
    })
    .await
    {
        return fail_after_cleanup(error, None, Some(&context), Some(&browser), &playwright).await;
    }
    let blocked_urls = Arc::new(Mutex::new(Vec::new()));
    if let Err(error) = operation("install browser network guard", async {
        context
            .route("**/*", {
                let blocked_urls = Arc::clone(&blocked_urls);
                move |route| {
                    let blocked_urls = Arc::clone(&blocked_urls);
                    async move {
                        let url = route.request().url().to_owned();
                        if url.starts_with("file:")
                            || url.starts_with("data:")
                            || url.starts_with("blob:")
                        {
                            route.continue_(None).await
                        } else {
                            blocked_urls
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .push(url);
                            route.abort(None).await
                        }
                    }
                }
            })
            .await
            .context("install browser network guard")
    })
    .await
    {
        return fail_after_cleanup(error, None, Some(&context), Some(&browser), &playwright).await;
    }
    let page = match operation("open Chromium page", async {
        context.new_page().await.context("open Chromium page")
    })
    .await
    {
        Ok(page) => page,
        Err(error) => {
            return fail_after_cleanup(error, None, Some(&context), Some(&browser), &playwright)
                .await;
        }
    };
    Ok(Session {
        playwright,
        browser,
        context,
        page,
        blocked_urls,
    })
}

impl Session {
    pub async fn finish(self) -> Result<()> {
        let blocked_urls = self
            .blocked_urls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        close_resources(
            Some(&self.page),
            Some(&self.context),
            Some(&self.browser),
            &self.playwright,
        )
        .await?;
        ensure!(
            blocked_urls.is_empty(),
            "Chromium requested external URLs: {}",
            blocked_urls.join(", ")
        );
        Ok(())
    }
}

async fn fail_after_cleanup<T>(
    error: anyhow::Error,
    page: Option<&Page>,
    context: Option<&BrowserContext>,
    browser: Option<&Browser>,
    playwright: &Playwright,
) -> Result<T> {
    match close_resources(page, context, browser, playwright).await {
        Ok(()) => Err(error),
        Err(cleanup_error) => {
            Err(error.context(format!("browser cleanup also failed: {cleanup_error:#}")))
        }
    }
}

async fn close_resources(
    page: Option<&Page>,
    context: Option<&BrowserContext>,
    browser: Option<&Browser>,
    playwright: &Playwright,
) -> Result<()> {
    let mut failures = Vec::new();
    if let Some(page) = page
        && let Err(error) = with_timeout("close Chromium page", CLEANUP_TIMEOUT, async {
            page.close().await.context("close Chromium page")
        })
        .await
    {
        failures.push(format!("page: {error:#}"));
    }
    if let Some(context) = context
        && let Err(error) = with_timeout("close Chromium context", CLEANUP_TIMEOUT, async {
            context.close().await.context("close Chromium context")
        })
        .await
    {
        failures.push(format!("context: {error:#}"));
    }
    if let Some(browser) = browser
        && let Err(error) = with_timeout("close Chromium browser", CLEANUP_TIMEOUT, async {
            browser.close().await.context("close Chromium browser")
        })
        .await
    {
        failures.push(format!("browser: {error:#}"));
    }
    if let Err(error) = with_timeout("shut down Playwright driver", CLEANUP_TIMEOUT, async {
        playwright
            .shutdown()
            .await
            .context("shut down Playwright driver")
    })
    .await
    {
        failures.push(format!("driver: {error:#}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!("Chromium cleanup failed: {}", failures.join("; "));
    }
}

#[cfg(test)]
mod tests {
    use std::{future, time::Duration};

    use super::with_timeout;

    #[tokio::test]
    async fn deadline_rejects_an_operation_that_never_finishes() {
        let error = with_timeout(
            "pending browser operation",
            Duration::from_millis(1),
            future::pending::<anyhow::Result<()>>(),
        )
        .await
        .expect_err("pending operation must time out");

        assert!(
            format!("{error:#}").contains("pending browser operation timed out after 1ms"),
            "unexpected deadline error: {error:#}"
        );
    }
}
