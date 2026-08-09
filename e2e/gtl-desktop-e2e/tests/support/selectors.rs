use anyhow::{Context, Result};
use thirtyfour::{By, WebDriver, WebElement};

use super::wait::{self, ASSERTION_TIMEOUT};

const ACCESSIBLE_NAME_SCRIPT: &str = r#"
const expected = arguments[0];
const scopeSelector = arguments[1];
const normalize = (value) => (value ?? "").replace(/\s+/g, " ").trim();
const elementRoot = (element) => element.getRootNode();
const labelledByText = (element) => (element.getAttribute("aria-labelledby") ?? "")
  .split(/\s+/).filter(Boolean)
  .map((id) => elementRoot(element).getElementById?.(id)?.textContent ?? "").join(" ");
const associatedLabelText = (element) => (element instanceof HTMLInputElement ||
  element instanceof HTMLSelectElement || element instanceof HTMLTextAreaElement)
  ? Array.from(element.labels ?? []).map((label) => label.textContent ?? "").join(" ") : "";
const accessibleName = (element) => normalize(element.getAttribute("aria-label")) ||
  normalize(labelledByText(element)) || normalize(associatedLabelText(element)) ||
  normalize(element.closest("label")?.textContent) || normalize(element.getAttribute("alt")) ||
  ((element instanceof HTMLButtonElement || element instanceof HTMLAnchorElement ||
    element.hasAttribute("role")) ? normalize(element.textContent) : "");
const displayed = (element) => {
  const style = window.getComputedStyle(element);
  return style.display !== "none" && style.visibility !== "hidden" && element.getClientRects().length > 0;
};
const selector = "[aria-label], [aria-labelledby], input, select, textarea, img, button, a, [role]";
const findIn = (root) => {
  const match = Array.from(root.querySelectorAll(selector))
    .find((element) => displayed(element) && accessibleName(element) === expected);
  if (match) return match;
  for (const element of root.querySelectorAll("*")) {
    if (!element.shadowRoot) continue;
    const nested = findIn(element.shadowRoot);
    if (nested) return nested;
  }
  return null;
};
const scope = typeof scopeSelector === "string" ? document.querySelector(scopeSelector) : document;
if (!scope) return null;
return findIn(scope);
"#;

pub async fn by_accessible_name(driver: &WebDriver, expected: &str) -> Result<WebElement> {
    wait::until(
        &format!("displayed element with accessible name {expected:?}"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    ACCESSIBLE_NAME_SCRIPT,
                    vec![serde_json::json!(expected), serde_json::Value::Null],
                )
                .await
                .with_context(|| format!("find element with accessible name {expected:?}"))?;
            if result.json().is_null() {
                return Ok(None);
            }
            result
                .element()
                .map(Some)
                .with_context(|| format!("convert accessible element {expected:?}"))
        },
    )
    .await
}

pub async fn by_accessible_name_within(
    driver: &WebDriver,
    scope_selector: &str,
    expected: &str,
) -> Result<WebElement> {
    wait::until(
        &format!("displayed element with accessible name {expected:?} within {scope_selector}"),
        ASSERTION_TIMEOUT,
        || async {
            let result = driver
                .execute(
                    ACCESSIBLE_NAME_SCRIPT,
                    vec![
                        serde_json::json!(expected),
                        serde_json::json!(scope_selector),
                    ],
                )
                .await
                .with_context(|| {
                    format!(
                        "find element with accessible name {expected:?} within {scope_selector}"
                    )
                })?;
            if result.json().is_null() {
                return Ok(None);
            }
            result.element().map(Some).with_context(|| {
                format!("convert accessible element {expected:?} within {scope_selector}")
            })
        },
    )
    .await
}

pub async fn by_css(driver: &WebDriver, selector: &str, description: &str) -> Result<WebElement> {
    wait::until(description, ASSERTION_TIMEOUT, || async {
        Ok(driver.find_all(By::Css(selector)).await?.into_iter().next())
    })
    .await
}
