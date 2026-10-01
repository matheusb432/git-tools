use std::{fs, path::Path};

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use thirtyfour::{By, WebDriver};

use super::{SNAPSHOT_SCRIPT, THEMES, WIDTHS};
use crate::support;

const SETTLE_SCRIPT: &str = r"
const done = arguments[arguments.length - 1];
let frames = 0, stable = 0, previous;
document.fonts.ready.then(() => requestAnimationFrame(function settle() {
    const elements = [...document.querySelectorAll('#main *:not(svg):not(svg *)')];
    const geometry = JSON.stringify(elements.map(element => {
        const box = element.getBoundingClientRect();
        return [box.x, box.y, box.width, box.height, element.textContent, element.getAttribute('aria-busy')];
    }));
    const moving = document.getAnimations().some(animation => {
        const timing = animation.effect?.getComputedTiming();
        return animation.playState === 'running' && Number.isFinite(timing?.endTime);
    });
    stable = !moving && geometry === previous ? stable + 1 : 0;
    previous = geometry;
    if (++frames > 600) return done({error: 'page did not settle within 600 frames'});
    if (stable < 8) return requestAnimationFrame(settle);
    done({frames});
}));
";

pub(super) async fn capture(driver: &WebDriver, output: &Path) -> Result<Vec<String>> {
    driver
        .execute(
            "document.documentElement.dataset.reduceMotion = 'false';",
            Vec::new(),
        )
        .await?;
    support::click(driver, By::Css("button[aria-label='Settings']")).await?;
    let mut cases = Vec::new();
    for section in ["appearance", "locale", "snapshots", "git"] {
        support::click(
            driver,
            By::Css(format!("[data-settings-section='{section}']")),
        )
        .await?;
        for theme in THEMES {
            driver
                .execute(
                    "document.documentElement.dataset.theme = arguments[0];",
                    vec![json!(theme)],
                )
                .await?;
            for width in WIDTHS {
                driver.set_window_rect(20, 20, width, 800).await?;
                driver.action_chain().move_to(0, 0).perform().await?;
                driver
                    .execute(
                        "document.activeElement?.blur(); document.querySelector('.settings-content').scrollTop = 0;",
                        Vec::new(),
                    )
                    .await?;
                let name = format!("settings-{section}-{theme}-{width}");
                save(driver, output, &name).await?;
                cases.push(name);
            }
        }
    }
    Ok(cases)
}

async fn save(driver: &WebDriver, output: &Path, name: &str) -> Result<()> {
    let settled = driver
        .execute_async(SETTLE_SCRIPT, Vec::new())
        .await?
        .convert::<Value>()?;
    ensure!(
        settled["error"].is_null(),
        "page settlement failed: {settled}"
    );
    let snapshot = driver
        .execute(
            SNAPSHOT_SCRIPT,
            vec![
                json!(".settings-page-shell"),
                json!("#main *:not(svg):not(svg *)"),
            ],
        )
        .await?
        .convert::<Value>()?;
    let bytes = serde_json::to_vec(&snapshot)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "page capture exceeded 16 MiB"
    );
    fs::write(output.join(format!("{name}.json")), bytes)?;
    driver
        .screenshot(&output.join(format!("{name}.png")))
        .await?;
    Ok(())
}
