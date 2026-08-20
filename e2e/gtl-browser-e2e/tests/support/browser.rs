use std::{
    error::Error,
    fmt,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use playwright_rs::{
    LaunchOptions, Playwright,
    protocol::{Browser, BrowserContext, CDPSession, GotoOptions, Page},
};

pub const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);

const AUDIT_OBSERVATIONS_MAX: usize = 32;
const AUDIT_OBSERVATION_CHARACTERS_MAX: usize = 512;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);
const DEBUGGER_EVENT_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);
const DEBUGGER_EVENT_QUIET_PERIOD: Duration = Duration::from_millis(25);
const DEBUGGER_EVENT_STABLE_INTERVALS: usize = 2;

pub struct Session {
    playwright: Playwright,
    browser: Browser,
    pub context: BrowserContext,
    pub page: Page,
    artifact_audit: ArtifactAudit,
}

struct ArtifactAudit {
    debugger: CDPSession,
    requests: Arc<Mutex<RequestAuditState>>,
    scripts: Arc<Mutex<ScriptAuditState>>,
}

#[derive(Clone, Debug)]
struct BoundedObservations<T> {
    entries: Vec<T>,
    omitted: usize,
}

impl<T> Default for BoundedObservations<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            omitted: 0,
        }
    }
}

impl<T> BoundedObservations<T> {
    fn record(&mut self, observation: T) {
        if self.entries.len() < AUDIT_OBSERVATIONS_MAX {
            self.entries.push(observation);
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.omitted == 0
    }
}

#[derive(Clone, Debug)]
struct RequestObservation {
    is_navigation: bool,
    is_top_level: bool,
    method: String,
    resource_type: String,
    url: String,
}

impl fmt::Display for RequestObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {} {} (navigation: {}, top-level: {})",
            self.method, self.resource_type, self.url, self.is_navigation, self.is_top_level
        )
    }
}

#[derive(Clone, Debug)]
struct WebAssemblyScriptObservation {
    script_id: String,
    url: String,
}

impl fmt::Display for WebAssemblyScriptObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let url = if self.url.is_empty() {
            "<anonymous>"
        } else {
            &self.url
        };
        write!(formatter, "script {} at {url}", self.script_id)
    }
}

#[derive(Clone, Debug, Default)]
struct RequestAuditState {
    expected_document_url: Option<String>,
    permitted_document_requests: usize,
    unexpected_requests: BoundedObservations<RequestObservation>,
}

impl RequestAuditState {
    fn configure(&mut self, url: &str) -> std::result::Result<(), ArtifactAuditError> {
        if !url.starts_with("file://") {
            return Err(ArtifactAuditError::ArtifactUrlIsNotFile {
                url: bounded_text(url),
            });
        }
        if let Some(configured_url) = &self.expected_document_url {
            return Err(ArtifactAuditError::ArtifactUrlAlreadyConfigured {
                configured_url: bounded_text(configured_url),
                requested_url: bounded_text(url),
            });
        }
        self.expected_document_url = Some(url.to_owned());
        Ok(())
    }

    fn observe(
        &mut self,
        url: &str,
        method: &str,
        resource_type: &str,
        is_navigation: bool,
        is_top_level: bool,
    ) -> bool {
        let is_permitted_document = self.expected_document_url.as_deref() == Some(url)
            && method == "GET"
            && resource_type == "document"
            && is_navigation
            && is_top_level
            && self.permitted_document_requests == 0;
        if is_permitted_document {
            self.permitted_document_requests = 1;
            return true;
        }
        self.unexpected_requests.record(RequestObservation {
            is_navigation,
            is_top_level,
            method: bounded_text(method),
            resource_type: bounded_text(resource_type),
            url: bounded_text(url),
        });
        false
    }
}

#[derive(Clone, Debug, Default)]
struct ScriptAuditState {
    parsed_events: usize,
    webassembly_scripts: BoundedObservations<WebAssemblyScriptObservation>,
}

#[derive(Debug)]
enum ArtifactAuditError {
    ArtifactUrlAlreadyConfigured {
        configured_url: String,
        requested_url: String,
    },
    ArtifactUrlIsNotFile {
        url: String,
    },
    DebuggerEventsDidNotSettle {
        observed_events: usize,
    },
    Violations {
        expected_document: Option<String>,
        unexpected_requests: BoundedObservations<RequestObservation>,
        webassembly_scripts: BoundedObservations<WebAssemblyScriptObservation>,
    },
}

impl fmt::Display for ArtifactAuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArtifactUrlAlreadyConfigured {
                configured_url,
                requested_url,
            } => write!(
                formatter,
                "artifact navigation is already configured for {configured_url}; rejected {requested_url}"
            ),
            Self::ArtifactUrlIsNotFile { url } => {
                write!(
                    formatter,
                    "artifact navigation requires a file:// URL, got {url}"
                )
            }
            Self::DebuggerEventsDidNotSettle { observed_events } => write!(
                formatter,
                "Chromium Debugger events did not settle within {}ms after {observed_events} scriptParsed events",
                DEBUGGER_EVENT_DRAIN_TIMEOUT.as_millis()
            ),
            Self::Violations {
                expected_document,
                unexpected_requests,
                webassembly_scripts,
            } => {
                write!(formatter, "offline artifact audit failed")?;
                if let Some(url) = expected_document {
                    write!(
                        formatter,
                        "; requested top-level document was not loaded exactly once: {url}"
                    )?;
                }
                write_observations(formatter, "unexpected request", unexpected_requests)?;
                write_observations(
                    formatter,
                    "WebAssembly scriptParsed event",
                    webassembly_scripts,
                )
            }
        }
    }
}

impl Error for ArtifactAuditError {}

fn write_observations<T: fmt::Display>(
    formatter: &mut fmt::Formatter<'_>,
    label: &str,
    observations: &BoundedObservations<T>,
) -> fmt::Result {
    if observations.is_empty() {
        return Ok(());
    }
    write!(formatter, "; {label}s: ")?;
    for (index, observation) in observations.entries.iter().enumerate() {
        if index > 0 {
            write!(formatter, ", ")?;
        }
        write!(formatter, "{observation}")?;
    }
    if observations.omitted > 0 {
        write!(formatter, " (+{} omitted)", observations.omitted)?;
    }
    Ok(())
}

fn bounded_text(value: &str) -> String {
    let mut characters = value.chars();
    let bounded = characters
        .by_ref()
        .take(AUDIT_OBSERVATION_CHARACTERS_MAX)
        .collect::<String>();
    if characters.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
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
    let artifact_audit = match ArtifactAudit::install(&context, &page).await {
        Ok(audit) => audit,
        Err(error) => {
            return fail_after_cleanup(
                error,
                Some(&page),
                Some(&context),
                Some(&browser),
                &playwright,
            )
            .await;
        }
    };
    Ok(Session {
        playwright,
        browser,
        context,
        page,
        artifact_audit,
    })
}

impl Session {
    pub async fn navigate_to_artifact(&self, url: &str) -> Result<()> {
        self.artifact_audit.configure_artifact_url(url)?;
        operation("navigate to offline artifact", async {
            self.page
                .goto(url, GotoOptions::new().timeout(OPERATION_TIMEOUT))
                .await
                .context("navigate to offline artifact")
                .map(|_| ())
        })
        .await
    }

    pub async fn verify_artifact_audit(&self) -> Result<()> {
        self.artifact_audit.verify_current().await
    }

    pub async fn finish(self) -> Result<()> {
        let audit_result = self.artifact_audit.finish().await;
        let cleanup_result = close_resources(
            Some(&self.page),
            Some(&self.context),
            Some(&self.browser),
            &self.playwright,
        )
        .await;
        attach_secondary_error(audit_result, cleanup_result, "browser cleanup")
    }
}

impl ArtifactAudit {
    async fn install(context: &BrowserContext, page: &Page) -> Result<Self> {
        let requests = Arc::new(Mutex::new(RequestAuditState::default()));
        operation("install exact artifact request audit", async {
            context
                .route("**/*", {
                    let requests = Arc::clone(&requests);
                    move |route| {
                        let requests = Arc::clone(&requests);
                        async move {
                            let request = route.request();
                            let is_top_level = request
                                .frame()
                                .is_some_and(|frame| frame.parent_frame().is_none());
                            let permit = requests
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .observe(
                                    request.url(),
                                    request.method(),
                                    request.resource_type(),
                                    request.is_navigation_request(),
                                    is_top_level,
                                );
                            if permit {
                                route.continue_(None).await
                            } else {
                                route.abort(None).await
                            }
                        }
                    }
                })
                .await
                .context("install exact artifact request audit")
        })
        .await?;

        let debugger = operation("attach Chromium Debugger audit", async {
            context
                .new_cdp_session(page)
                .await
                .context("attach Chromium Debugger audit")
        })
        .await?;
        let scripts = Arc::new(Mutex::new(ScriptAuditState::default()));
        debugger.on("Debugger.scriptParsed", {
            let scripts = Arc::clone(&scripts);
            move |event| {
                let scripts = Arc::clone(&scripts);
                async move {
                    let mut scripts = scripts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    scripts.parsed_events = scripts.parsed_events.saturating_add(1);
                    if event.get("scriptLanguage").and_then(|value| value.as_str())
                        == Some("WebAssembly")
                    {
                        scripts
                            .webassembly_scripts
                            .record(WebAssemblyScriptObservation {
                                script_id: bounded_text(
                                    event
                                        .get("scriptId")
                                        .and_then(|value| value.as_str())
                                        .unwrap_or("<unknown>"),
                                ),
                                url: bounded_text(
                                    event
                                        .get("url")
                                        .and_then(|value| value.as_str())
                                        .unwrap_or_default(),
                                ),
                            });
                    }
                    Ok(())
                }
            }
        });
        operation("enable Chromium Debugger audit", async {
            debugger
                .send("Debugger.enable", None)
                .await
                .context("enable Chromium Debugger audit")?;
            Ok(())
        })
        .await?;

        Ok(Self {
            debugger,
            requests,
            scripts,
        })
    }

    fn configure_artifact_url(&self, url: &str) -> std::result::Result<(), ArtifactAuditError> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .configure(url)
    }

    async fn verify_current(&self) -> Result<()> {
        self.wait_for_debugger_events().await?;
        self.verify_observations()?;
        Ok(())
    }

    async fn finish(&self) -> Result<()> {
        let disable_result = operation("disable Chromium Debugger audit", async {
            self.debugger
                .send("Debugger.disable", None)
                .await
                .context("disable Chromium Debugger audit")?;
            Ok(())
        })
        .await;
        let settle_result = self.wait_for_debugger_events().await.map_err(Into::into);
        let verification_result = self.verify_observations().map_err(Into::into);
        let detach_result = operation("detach Chromium Debugger audit", async {
            self.debugger
                .detach()
                .await
                .context("detach Chromium Debugger audit")
        })
        .await;

        let result =
            attach_secondary_error(disable_result, settle_result, "Debugger event observation");
        let result = attach_secondary_error(result, verification_result, "artifact audit");
        attach_secondary_error(result, detach_result, "Debugger audit cleanup")
    }

    async fn wait_for_debugger_events(&self) -> std::result::Result<(), ArtifactAuditError> {
        // playwright-rs dispatches CDP callbacks on spawned tasks. Requiring two
        // quiet intervals gives preceding scriptParsed callbacks time to record,
        // while the outer deadline keeps the observation finite.
        let deadline = Instant::now() + DEBUGGER_EVENT_DRAIN_TIMEOUT;
        let mut previous_count = self.parsed_event_count();
        let mut stable_intervals = 0;
        loop {
            tokio::time::sleep(DEBUGGER_EVENT_QUIET_PERIOD).await;
            let observed_events = self.parsed_event_count();
            if observed_events == previous_count {
                stable_intervals += 1;
                if stable_intervals == DEBUGGER_EVENT_STABLE_INTERVALS {
                    return Ok(());
                }
            } else {
                previous_count = observed_events;
                stable_intervals = 0;
            }
            if Instant::now() >= deadline {
                return Err(ArtifactAuditError::DebuggerEventsDidNotSettle { observed_events });
            }
        }
    }

    fn parsed_event_count(&self) -> usize {
        self.scripts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .parsed_events
    }

    fn verify_observations(&self) -> std::result::Result<(), ArtifactAuditError> {
        let requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let scripts = self
            .scripts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let expected_document = requests
            .expected_document_url
            .filter(|_| requests.permitted_document_requests != 1)
            .map(|url| bounded_text(&url));
        if expected_document.is_none()
            && requests.unexpected_requests.is_empty()
            && scripts.webassembly_scripts.is_empty()
        {
            return Ok(());
        }
        Err(ArtifactAuditError::Violations {
            expected_document,
            unexpected_requests: requests.unexpected_requests,
            webassembly_scripts: scripts.webassembly_scripts,
        })
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
    use super::{AUDIT_OBSERVATIONS_MAX, ArtifactAuditError, RequestAuditState};

    #[test]
    fn request_audit_permits_only_one_exact_top_level_file_document() {
        let artifact_url = "file:///tmp/raw-artifact.html";
        let mut audit = RequestAuditState::default();
        audit
            .configure(artifact_url)
            .expect("configure exact artifact URL");

        assert!(audit.observe(artifact_url, "GET", "document", true, true));
        for (url, method, resource_type, is_navigation, is_top_level) in [
            (artifact_url, "GET", "document", true, true),
            (artifact_url, "POST", "document", true, true),
            ("file:///tmp/sibling.css", "GET", "stylesheet", false, true),
            (
                "data:text/javascript,void%200",
                "GET",
                "script",
                false,
                true,
            ),
            ("blob:null/example", "GET", "script", false, true),
            (artifact_url, "GET", "script", false, true),
            (artifact_url, "GET", "document", true, false),
        ] {
            assert!(!audit.observe(url, method, resource_type, is_navigation, is_top_level));
        }

        assert_eq!(audit.permitted_document_requests, 1);
        assert_eq!(audit.unexpected_requests.entries.len(), 7);
        assert_eq!(audit.unexpected_requests.omitted, 0);
    }

    #[test]
    fn request_audit_rejects_non_file_urls_and_reconfiguration() {
        let mut audit = RequestAuditState::default();
        assert!(matches!(
            audit.configure("https://example.invalid/artifact.html"),
            Err(ArtifactAuditError::ArtifactUrlIsNotFile { .. })
        ));
        audit
            .configure("file:///tmp/raw-artifact.html")
            .expect("configure file artifact URL");
        assert!(matches!(
            audit.configure("file:///tmp/other-artifact.html"),
            Err(ArtifactAuditError::ArtifactUrlAlreadyConfigured { .. })
        ));
    }

    #[test]
    fn request_audit_bounds_violation_observations() {
        let mut audit = RequestAuditState::default();
        for index in 0..AUDIT_OBSERVATIONS_MAX + 3 {
            let url = format!("file:///tmp/sibling-{index}.css");
            assert!(!audit.observe(&url, "GET", "stylesheet", false, true));
        }

        assert_eq!(
            audit.unexpected_requests.entries.len(),
            AUDIT_OBSERVATIONS_MAX
        );
        assert_eq!(audit.unexpected_requests.omitted, 3);
    }
}
