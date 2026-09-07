use std::{future::Future, panic::AssertUnwindSafe, path::PathBuf, pin::Pin};

use anyhow::{Context, Result, ensure};
use futures_util::FutureExt;
use serde::Deserialize;
use serde_json::json;
use thirtyfour::{By, WebDriver};

pub mod catalogue;
pub mod evidence;
pub mod fixture;
pub mod selectors;
pub mod session;
pub mod wait;

#[derive(Deserialize)]
struct CopyObservation {
    text: String,
    prevented: bool,
}

pub async fn copy_selected_diff_line(
    driver: &WebDriver,
    path: &str,
    marker: &str,
) -> Result<String> {
    let result = driver
        .execute(
            r#"
                const [path, marker] = arguments;
                const file = document.querySelector(
                    `[data-gtl-diff-file][data-path='${path}']`,
                );
                const source = [...file.querySelectorAll("[data-gtl-copy-text]")]
                    .find((element) => element.textContent.includes(marker));
                const range = document.createRange();
                range.selectNodeContents(source);
                const selection = window.getSelection();
                selection.removeAllRanges();
                selection.addRange(range);

                const clipboard = new DataTransfer();
                const event = new ClipboardEvent("copy", {
                    bubbles: true,
                    cancelable: true,
                    clipboardData: clipboard,
                });
                source.dispatchEvent(event);
                return {
                    text: clipboard.getData("text/plain"),
                    prevented: event.defaultPrevented,
                };
            "#,
            vec![json!(path), json!(marker)],
        )
        .await
        .context("copy a selected desktop diff line")?;
    let observation: CopyObservation = result
        .convert()
        .context("decode the desktop copy observation")?;

    ensure!(
        observation.prevented,
        "the desktop viewer did not intercept native copy"
    );
    Ok(observation.text)
}

pub async fn wait_for_active_diff(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
) -> Result<()> {
    wait::until(
        &format!("{repository} diff containing {marker}"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let result = probe_active_diff(driver, repository, marker).await;
            match result {
                Err(error)
                    if matches!(
                        error
                            .downcast_ref::<thirtyfour::error::WebDriverError>()
                            .map(thirtyfour::error::WebDriverError::as_inner),
                        Some(thirtyfour::error::WebDriverErrorInner::StaleElementReference(_))
                    ) =>
                {
                    Ok(None)
                }
                result => result,
            }
        },
    )
    .await
}

async fn probe_active_diff(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
) -> Result<Option<()>> {
    let active_tabs = driver
        .find_all(By::Css("[role='tab'][aria-selected='true']"))
        .await?;
    let Some(active_tab) = active_tabs.into_iter().next() else {
        return Ok(None);
    };
    let title = active_tab.attr("title").await?.unwrap_or_default();
    if !active_tab.is_displayed().await?
        || !title.contains(repository)
        || active_tab.attr("aria-busy").await?.as_deref() == Some("true")
    {
        return Ok(None);
    }

    let main = driver.find(By::Css("main")).await?;
    if main.is_displayed().await? && main.text().await?.contains(marker) {
        return Ok(Some(()));
    }
    Ok(None)
}

pub async fn run_test<F>(name: &str, body: F) -> Result<()>
where
    F: for<'session> FnOnce(
        &'session mut session::TestSession,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'session>>,
{
    run_test_with_result(name, body).await
}

pub async fn run_test_with_result<T, F>(name: &str, body: F) -> Result<T>
where
    F: for<'session> FnOnce(
        &'session mut session::TestSession,
    ) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'session>>,
{
    let mut session = session::TestSession::start(name).await?;
    let body_result = AssertUnwindSafe(body(&mut session)).catch_unwind().await;
    let passed = matches!(&body_result, Ok(Ok(_)));
    let evidence_result = match session.driver_if_active() {
        Some(driver) => evidence::capture(driver, name, passed).await,
        None => Ok(None),
    };
    let cleanup_result = session.finish().await;

    match body_result {
        Ok(Ok(value)) => {
            attach_secondary_error(evidence_result.map(|_| ()), cleanup_result, "test cleanup")?;
            Ok(value)
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

fn attach_evidence<T>(primary: Result<T>, evidence: Result<Option<PathBuf>>) -> Result<T> {
    match (primary, evidence) {
        (Ok(value), Ok(_)) => Ok(value),
        (Ok(_), Err(error)) => Err(error),
        (Err(primary_error), Ok(Some(path))) => {
            Err(primary_error).context(format!("evidence saved to {}", path.display()))
        }
        (Err(primary_error), Ok(None)) => Err(primary_error),
        (Err(primary_error), Err(error)) => {
            Err(primary_error).context(format!("evidence capture also failed: {error:#}"))
        }
    }
}

fn attach_secondary_error<T>(primary: Result<T>, secondary: Result<()>, label: &str) -> Result<T> {
    match (primary, secondary) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(secondary_error)) => Err(secondary_error),
        (Err(primary_error), Ok(())) => Err(primary_error),
        (Err(primary_error), Err(secondary_error)) => {
            Err(primary_error).context(format!("{label} also failed: {secondary_error:#}"))
        }
    }
}
