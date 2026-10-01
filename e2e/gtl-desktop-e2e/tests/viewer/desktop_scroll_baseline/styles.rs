use std::{fmt::Write as _, fs, path::Path, time::Duration};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use thirtyfour::{By, WebDriver};

use super::{fixture_git, forward_diff, runner_environment};
use crate::support;

mod pages;

const VIEW_NAME: &str = "style-parity";
const MARKER: &str = "style-parity-marker";
const THEMES: [&str; 5] = ["dark", "mirage", "glacier", "graphite", "carbon"];
const WIDTHS: [u32; 4] = [1_440, 1_024, 760, 414];

const SETTLE_SCRIPT: &str = r"
const done = arguments[arguments.length - 1];
let frames = 0;
let previous = null, stable = 0;
document.fonts.ready.then(() => requestAnimationFrame(function settle() {
    const root = document.querySelector('[data-gtl-diff-document]');
    if (root) {root.scrollTop = 0; root.scrollLeft = 0;}
    for (const rows of document.querySelectorAll('.diff-rows')) rows.scrollLeft = 0;
    const windows = [...document.querySelectorAll('[data-gtl-row-window]')];
    const busy = windows.some(element => element.getAttribute('aria-busy') === 'true');
    const moving = document.getAnimations().some(animation => {
        const timing = animation.effect?.getComputedTiming();
        return animation.playState === 'running' && Number.isFinite(timing?.endTime);
    });
    const targets = [...document.querySelectorAll(
        '[data-gtl-diff-document], .diff-file-scroll, .diff-rows, .scrollbars, .scrollbar-rail, .scrollbar-thumb'
    )];
    const fingerprint = JSON.stringify(targets.map(element => {
        const box = element.getBoundingClientRect();
        return [box.x, box.y, box.width, box.height, element.scrollTop, element.scrollLeft,
            element.scrollWidth, element.scrollHeight, element.getAttribute('style'),
            element.getAttribute('aria-hidden'), element.getAttribute('aria-valuenow')];
    }));
    const scrollbarsReady = [...document.querySelectorAll('.scrollbars')].every(layer => {
        const viewport = layer.dataset.scrollTarget
            ? document.getElementById(layer.dataset.scrollTarget) : layer.parentElement;
        if (!viewport || !viewport.clientWidth || !viewport.clientHeight) return true;
        if (viewport.dataset.scrollEnhanced !== 'true') return false;
        const ready = [...layer.querySelectorAll('.scrollbar-rail')].every(rail => {
            const horizontal = rail.dataset.scrollAxis === 'horizontal';
            const maximum = horizontal ? viewport.scrollWidth - viewport.clientWidth : viewport.scrollHeight - viewport.clientHeight;
            return rail.getAttribute('aria-hidden') === String(maximum <= 0) &&
                Number(rail.getAttribute('aria-valuemax')) === Math.max(0, maximum);
        });
        if (!ready) viewport.dispatchEvent(new Event('scroll', {bubbles: true}));
        return ready;
    });
    stable = !busy && !moving && scrollbarsReady && fingerprint === previous ? stable + 1 : 0;
    previous = fingerprint;
    if (++frames > 600) return done({error: 'viewer did not settle within 600 frames', busy, moving, stable, scrollbarsReady,
        scrollbars: [...document.querySelectorAll('.scrollbars')].map(layer => {
            const viewport = layer.dataset.scrollTarget ? document.getElementById(layer.dataset.scrollTarget) : layer.parentElement;
            return {target: layer.dataset.scrollTarget, viewport: viewport ? [viewport.clientWidth, viewport.clientHeight,
                viewport.scrollWidth, viewport.scrollHeight, viewport.dataset.scrollEnhanced] : null,
                rails: [...layer.querySelectorAll('.scrollbar-rail')].map(rail => [rail.dataset.scrollAxis, rail.getAttribute('aria-hidden'), rail.getAttribute('aria-valuemax')])};
        })});
    if (stable < 8) return requestAnimationFrame(settle);
    done({frames});
}));
";

const SNAPSHOT_SCRIPT: &str = r"
const root = document.querySelector(arguments[0] || '[data-gtl-diff-document]');
const targets = [...document.querySelectorAll(
    arguments[1] || '.diff-file-scroll, .diff-rows, .diff-rows *, .diff-scrollbar-sticky, ' +
    '.diff-file-caret, .diff-workspace-grid, .diff-workspace-sidebar, .review-action-dock, ' +
    '.review-action-buttons, .review-action-reveal, .scrollbars, .scrollbar-rail, .scrollbar-thumb'
)];
if (!root || targets.length > 512) throw Error('style capture target bound exceeded or missing document');
const properties = element => {
    const style = getComputedStyle(element);
    return Object.fromEntries([...style].filter(name => !name.startsWith('--'))
        .sort().map(name => [name, style.getPropertyValue(name)]));
};
const geometry = element => {
    const box = element.getBoundingClientRect();
    return {x: box.x, y: box.y, width: box.width, height: box.height,
        scrollWidth: element.scrollWidth, scrollHeight: element.scrollHeight,
        clientWidth: element.clientWidth, clientHeight: element.clientHeight};
};
return {
    viewport: {width: innerWidth, height: innerHeight},
    theme: document.documentElement.dataset.theme,
    layout: root.dataset.layout, density: root.dataset.density,
    wrap: root.dataset.wrapLines,
    nodes: targets.map(element => ({
        tag: element.tagName, text: element.children.length ? null : element.textContent,
        row: element.getAttribute('data-gtl-diff-row'),
        aria_hidden: element.getAttribute('aria-hidden'),
        side: element.getAttribute('data-side'), tone: element.getAttribute('data-tone'),
        open: element.closest('[data-gtl-diff-file]')?.open ?? null,
        geometry: geometry(element), properties: properties(element),
        before: getComputedStyle(element, '::before').content,
        after: getComputedStyle(element, '::after').content,
    })),
};
";

const ARM_ANIMATIONS_SCRIPT: &str = r"
const expected = arguments[0];
const selectors = '.diff-file-caret, .diff-workspace-grid, .diff-workspace-sidebar, ' +
    '.sidebar-icon-panel, .sidebar-icon-divider, .review-action-buttons, .review-action-reveal';
const capture = window.styleAnimations = {ready: false, animations: [], frames: 0};
requestAnimationFrame(function observe() {
    if (++capture.frames > 600) {capture.error = 'animation action did not complete'; capture.ready = true; return;}
    if (!document.querySelector(expected)) return requestAnimationFrame(observe);
    capture.animations = document.getAnimations().filter(animation => animation.effect?.target?.matches(selectors));
    for (const animation of capture.animations) {animation.pause(); animation.currentTime = 0;}
    capture.ready = true;
});
";

const ANIMATIONS_SCRIPT: &str = r"
const capture = window.styleAnimations;
return {error: capture.error ?? null, animations: capture.animations.map(animation => ({
    target: animation.effect.target.tagName + '.' + animation.effect.target.classList[0],
    property: animation.transitionProperty ?? null,
    name: animation.animationName ?? null,
    timing: animation.effect.getTiming(), keyframes: animation.effect.getKeyframes(),
}))};
";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "run through the bounded desktop scroll style capture supervisor"]
async fn production_viewer_captures_style_states() -> Result<()> {
    capture(&runner_environment::required_environment_path(
        "GTL_DESKTOP_SCROLL_STYLES_PATH",
    )?)
    .await
}

async fn capture(output: &Path) -> Result<()> {
    ensure!(
        !output.exists(),
        "style capture directory already exists: {}",
        output.display()
    );
    fs::create_dir_all(output).context("create viewer style evidence directory")?;
    let output = output.to_path_buf();
    support::run_test("desktop-style-parity", |session| {
        Box::pin(async move {
            let repository = create_repository()?;
            forward_diff(&repository, session.data_root(), VIEW_NAME, "1")?;
            let driver = session.driver();
            driver.set_script_timeout(Duration::from_secs(30)).await?;
            support::wait_for_active_diff(driver, VIEW_NAME, MARKER).await?;
            let mut cases = capture_matrix(driver, &output).await?;
            configure(driver, "unified", "compact", false).await?;
            driver.set_window_rect(20, 20, 1_440, 800).await?;
            driver.execute("document.documentElement.dataset.theme = 'dark';", Vec::new()).await?;
            let animations = capture_motion(driver, &output).await?;
            let copied = support::copy_selected_diff_line(driver, "source.rs", MARKER).await?;
            ensure!(copied.contains(MARKER), "viewer copying omitted source marker: {copied:?}");
            driver.execute("getSelection().removeAllRanges();", Vec::new()).await?;
            cases.extend(pages::capture(driver, &output).await?);
            fs::write(output.join("manifest.json"), serde_json::to_vec_pretty(&json!({
                "cases": cases, "animations": animations, "copied": copied,
                "engine": "release Tauri WebKitGTK", "animation_samples_percent": [0, 25, 50, 75, 100],
            }))?)?;
            Ok(())
        })
    }).await
}

async fn capture_matrix(driver: &WebDriver, output: &Path) -> Result<Vec<String>> {
    let mut cases = Vec::new();
    for layout in ["unified", "split"] {
        for density in ["compact", "full"] {
            for wrap in [false, true] {
                cases.extend(capture_configuration(driver, output, layout, density, wrap).await?);
            }
        }
    }
    Ok(cases)
}

async fn capture_configuration(
    driver: &WebDriver,
    output: &Path,
    layout: &str,
    density: &str,
    wrap: bool,
) -> Result<Vec<String>> {
    configure(driver, layout, density, wrap).await?;
    let mut cases = Vec::new();
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
            driver.execute("document.querySelector('[data-gtl-diff-document]').scrollTop = 0; document.activeElement?.blur();", Vec::new()).await?;
            settle(driver).await?;
            let name = format!("{layout}-{density}-wrap-{wrap}-{theme}-{width}");
            save_snapshot(driver, output, &name).await?;
            cases.push(name);
        }
    }
    Ok(cases)
}

async fn capture_motion(driver: &WebDriver, output: &Path) -> Result<Vec<String>> {
    let mut animations = Vec::new();
    for reduced in [false, true] {
        driver
            .execute(
                "document.documentElement.dataset.reduceMotion = arguments[0];",
                vec![json!(reduced.to_string())],
            )
            .await?;
        for (name, trigger, expected) in [
            (
                "fold",
                "[data-path='source.rs'] > summary",
                "[data-path='source.rs']:not([open])",
            ),
            (
                "unfold",
                "[data-path='source.rs'] > summary",
                "[data-path='source.rs'][open]",
            ),
            (
                "hide-actions",
                "#review-actions-hide",
                ".review-action-dock[data-collapsed='true']",
            ),
            (
                "show-actions",
                "#review-actions-show",
                ".review-action-dock[data-collapsed='false']",
            ),
            (
                "hide-files",
                "#files-sidebar-toggle",
                "#files-sidebar-toggle[aria-pressed='false']",
            ),
            (
                "show-files",
                "#files-sidebar-toggle",
                "#files-sidebar-toggle[aria-pressed='true']",
            ),
        ] {
            animations
                .push(capture_action(driver, output, name, trigger, expected, reduced).await?);
        }
    }
    Ok(animations)
}

async fn capture_action(
    driver: &WebDriver,
    output: &Path,
    name: &str,
    trigger: &str,
    expected: &str,
    reduced: bool,
) -> Result<String> {
    settle(driver).await?;
    driver
        .execute(ARM_ANIMATIONS_SCRIPT, vec![json!(expected)])
        .await?;
    support::click(driver, By::Css(trigger)).await?;
    driver.action_chain().move_to(0, 0).perform().await?;
    support::wait::until(
        "capture active viewer animations",
        Duration::from_secs(15),
        || async {
            let ready = driver
                .execute("return window.styleAnimations.ready;", Vec::new())
                .await?
                .convert::<bool>()?;
            Ok(ready.then_some(()))
        },
    )
    .await?;
    let observation = driver
        .execute(ANIMATIONS_SCRIPT, Vec::new())
        .await?
        .convert::<Value>()?;
    ensure!(
        observation["error"].is_null(),
        "animation capture failed: {observation}"
    );
    let effects = observation["animations"]
        .as_array()
        .context("animation observations must be an array")?;
    ensure!(
        reduced == effects.is_empty(),
        "unexpected animations with reduce-motion={reduced}: {observation}"
    );
    let name = format!("motion-{name}-reduced-{reduced}");
    fs::write(
        output.join(format!("{name}-effects.json")),
        serde_json::to_vec_pretty(&observation)?,
    )?;
    for percent in [0, 25, 50, 75, 100] {
        driver.execute_async(r"
            const fraction = arguments[0], done = arguments[arguments.length - 1];
            const effects = window.styleAnimations.animations;
            const duration = Math.max(0, ...effects.map(animation => animation.effect.getComputedTiming().endTime));
            for (const animation of effects) animation.currentTime = duration * fraction;
            requestAnimationFrame(() => requestAnimationFrame(() => done(true)));
        ", vec![json!(f64::from(percent) / 100.0)]).await?;
        settle(driver).await?;
        save_snapshot(driver, output, &format!("{name}-{percent}")).await?;
    }
    driver
        .execute(
            "for (const animation of window.styleAnimations.animations) animation.finish();",
            Vec::new(),
        )
        .await?;
    Ok(name)
}

async fn configure(driver: &WebDriver, layout: &str, density: &str, wrap: bool) -> Result<()> {
    driver.set_window_rect(20, 20, 1_440, 800).await?;
    support::click(driver, By::Css("button[aria-label='Settings']")).await?;
    support::visible(driver, By::Css("[data-settings-section='snapshots']")).await?;
    support::click(driver, By::Css("[data-settings-section='snapshots']")).await?;
    for (name, value) in [("settings-layout", layout), ("settings-density", density)] {
        let input = support::visible(
            driver,
            By::Css(format!("input[name='{name}'][value='{value}']")),
        )
        .await?;
        if !input.is_selected().await? {
            input.click().await?;
        }
    }
    let input = support::visible(driver, By::Id("settings-wrap-lines")).await?;
    if input.is_selected().await? != wrap {
        input.click().await?;
    }
    support::click(driver, By::Css("button[aria-label='Back']")).await?;
    support::visible(driver, By::Css(format!("[data-gtl-diff-document][data-layout='{layout}'][data-density='{density}'][data-wrap-lines='{wrap}']"))).await?;
    support::wait_for_active_diff(driver, VIEW_NAME, MARKER).await
}

async fn settle(driver: &WebDriver) -> Result<()> {
    let result = driver
        .execute_async(SETTLE_SCRIPT, Vec::new())
        .await?
        .convert::<Value>()?;
    ensure!(
        result["error"].is_null(),
        "viewer settlement failed: {result}"
    );
    Ok(())
}

async fn save_snapshot(driver: &WebDriver, output: &Path, name: &str) -> Result<()> {
    let snapshot = driver
        .execute(SNAPSHOT_SCRIPT, Vec::new())
        .await?
        .convert::<Value>()?;
    let bytes = serde_json::to_vec(&snapshot)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "viewer style capture exceeded 16 MiB"
    );
    fs::write(output.join(format!("{name}.json")), bytes)?;
    driver
        .screenshot(&output.join(format!("{name}.png")))
        .await?;
    Ok(())
}

fn create_repository() -> Result<std::path::PathBuf> {
    let repository = runner_environment::required_environment_path("GTL_E2E_FIXTURE_ROOT")?
        .join("style-parity-fixture");
    fs::create_dir_all(&repository)?;
    fixture_git(&repository, &["init", "-q", "-b", "main"])?;
    fixture_git(
        &repository,
        &["config", "user.name", "Style Parity Fixture"],
    )?;
    fixture_git(
        &repository,
        &["config", "user.email", "style-parity@example.invalid"],
    )?;
    let mut context = String::new();
    for number in 0..12 {
        writeln!(context, "    // unchanged context {number}")?;
    }
    let before = format!(
        "pub fn render() {{\n    let value = 7;\n{context}    println!(\"old text\");\n}}\n"
    );
    fs::write(repository.join("source.rs"), before)?;
    fixture_git(&repository, &["add", "source.rs"])?;
    fixture_git(
        &repository,
        &["commit", "-q", "-m", "fixture: add source baseline"],
    )?;
    fixture_git(&repository, &["switch", "-q", "-c", "feature"])?;
    let after = format!(
        "pub fn render() {{\n    let value = 42;\n    // {MARKER}: áβ中\n{context}    println!(\"{}\");\n    println!(\"new text\");\n}}",
        "long source line ".repeat(90)
    );
    fs::write(repository.join("source.rs"), after)?;
    fixture_git(&repository, &["add", "source.rs"])?;
    fixture_git(
        &repository,
        &[
            "commit",
            "-q",
            "-m",
            "fixture: modify syntax and long lines",
        ],
    )?;
    Ok(repository)
}
