use std::time::Duration;

use anyhow::{Context as _, Result};
use playwright_rs::{
    expect,
    protocol::{EmulateMediaOptions, Page, ReducedMotion, Viewport},
};

use crate::{harness::evidence::Recording, support};

#[tokio::test(flavor = "multi_thread")]
async fn guided_tour_tracks_targets_and_keeps_keyboard_navigation_usable() -> Result<()> {
    let spec = support::Spec::start_web("guided-tour").await?;
    let outcome = async {
        let page = &spec.session.page;
        let base = std::env::var("GTL_COMPONENT_PREVIEW_URL").context("component preview URL")?;
        page.set_viewport_size(Viewport {
            width: 1000,
            height: 780,
        })
        .await?;
        page.goto(&format!("{base}/render/guided-tour/interactive"), None)
            .await?;
        let guide = page.locator("[data-tour-launch='preview-guided-tour']");
        let dialog = page.locator("dialog[data-guided-tour]");
        expect(guide.locator(".guided-tour-unseen"))
            .to_have_count(1)
            .await?;
        launch(page).await?;
        expect_spotlight(page, "preview-tour-first").await?;
        for _ in 0..8 {
            page.keyboard().press("Tab", None).await?;
            expect(dialog.locator(":focus")).to_have_count(1).await?;
        }
        page.keyboard().press("ArrowRight", None).await?;
        expect_spotlight(page, "preview-tour-second").await?;
        page.set_viewport_size(Viewport {
            width: 320,
            height: 240,
        })
        .await?;
        expect_card_in_viewport(page, 320.0, 240.0).await?;
        page.keyboard().press("ArrowLeft", None).await?;
        expect_spotlight(page, "preview-tour-first").await?;
        page.keyboard().press("Escape", None).await?;
        expect(dialog.clone()).to_have_count(0).await?;
        expect(guide.clone()).to_be_focused().await?;
        expect(guide.locator(".guided-tour-unseen"))
            .to_have_count(0)
            .await?;

        launch(page).await?;
        expect_spotlight(page, "preview-tour-first").await?;
        for _ in 0..2 {
            page.keyboard().press("ArrowRight", None).await?;
        }
        expect(page.locator(".guided-tour-spotlight"))
            .to_have_css("width", "0px")
            .await?;
        expect_card_in_viewport(page, 320.0, 240.0).await?;
        Recording::new(&spec.session, "guided-tour-missing-target")
            .finish(true)
            .await?;
        page.keyboard().press("ArrowRight", None).await?;
        expect(page.locator(".guided-tour-spotlight"))
            .to_have_css("width", "0px")
            .await?;
        page.keyboard().press("ArrowRight", None).await?;
        expect_spotlight(page, "preview-tour-offscreen").await?;
        page.keyboard().press("ArrowRight", None).await?;
        expect(page.locator(".guided-tour-actions button:last-child"))
            .to_have_text("Finish")
            .await?;
        expect_card_in_viewport(page, 320.0, 240.0).await?;
        page.locator(".guided-tour-copy").hover(None).await?;
        page.mouse().wheel(0.0, 600.0).await?;
        expect(page.locator(".guided-tour-actions button:last-child"))
            .to_be_visible()
            .await?;
        Recording::new(&spec.session, "guided-tour-long-copy")
            .finish(true)
            .await?;
        page.locator(".guided-tour-actions button:last-child")
            .click(None)
            .await?;
        expect(dialog.clone()).to_have_count(0).await?;
        expect(guide.clone()).to_be_focused().await?;

        page.reload(None).await?;
        expect(guide.locator(".guided-tour-unseen"))
            .to_have_count(0)
            .await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

async fn launch(page: &Page) -> Result<()> {
    page.locator("[data-tour-launch='preview-guided-tour']")
        .click(None)
        .await?;
    expect(page.locator("dialog[data-guided-tour]"))
        .to_be_visible()
        .await?;
    expect(page.locator(".guided-tour-actions button:last-child"))
        .to_be_focused()
        .await?;
    Ok(())
}

async fn expect_spotlight(page: &Page, anchor: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let target = page
                .locator(format!("[data-tour='{anchor}']:visible"))
                .bounding_box()
                .await?
                .context("tour target bounds")?;
            let spotlight = page
                .locator(".guided-tour-spotlight")
                .bounding_box()
                .await?
                .context("spotlight bounds")?;
            if spotlight.x <= target.x
                && spotlight.y <= target.y
                && spotlight.x + spotlight.width >= target.x + target.width
                && spotlight.y + spotlight.height >= target.y + target.height
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .context("spotlight did not follow its rendered target")?
}

#[tokio::test(flavor = "multi_thread")]
async fn guided_tour_uses_localized_controls_theme_tokens_and_reduced_motion() -> Result<()> {
    let spec = support::Spec::start_web("guided-tour-appearance").await?;
    let outcome = async {
        let page = &spec.session.page;
        let base = std::env::var("GTL_COMPONENT_PREVIEW_URL").context("component preview URL")?;
        page.goto(&format!("{base}/render/guided-tour/interactive"), None)
            .await?;
        page.emulate_media(
            EmulateMediaOptions::builder()
                .reduced_motion(ReducedMotion::Reduce)
                .build(),
        )
        .await?;
        let guide = page.locator("[data-tour-launch='preview-guided-tour']");
        launch(page).await?;
        expect(page.locator(".guided-tour-title"))
            .to_have_text("Read the file header")
            .await?;
        expect(page.locator(".guided-tour-actions button:last-child"))
            .to_have_text("Next")
            .await?;
        expect(page.locator(".guided-tour-spotlight"))
            .to_have_css("transition-duration", "0s")
            .await?;
        page.locator("button[aria-label='Close guide']")
            .click(None)
            .await?;
        support::get_button(page, "Use light palette")
            .click(None)
            .await?;
        launch(page).await?;
        expect(page.locator(".guided-tour-card"))
            .to_have_css("background-color", "rgb(255, 255, 255)")
            .await?;
        Recording::new(&spec.session, "guided-tour-light-english")
            .finish(true)
            .await?;
        page.locator("button[aria-label='Close guide']")
            .click(None)
            .await?;
        support::get_button(page, "Português").click(None).await?;
        launch(page).await?;
        expect(page.locator(".guided-tour-title"))
            .to_have_text("Ler o cabeçalho do arquivo")
            .await?;
        expect(page.locator(".guided-tour-actions button:last-child"))
            .to_have_text("Próxima")
            .await?;
        expect(page.locator(".guided-tour-card"))
            .to_have_css("background-color", "rgb(255, 255, 255)")
            .await?;
        Recording::new(&spec.session, "guided-tour-light-portuguese")
            .finish(true)
            .await?;
        page.locator("button[aria-label='Fechar guia']")
            .click(None)
            .await?;
        support::get_button(page, "Use dark palette")
            .click(None)
            .await?;
        launch(page).await?;
        expect(page.locator(".guided-tour-title"))
            .to_have_text("Ler o cabeçalho do arquivo")
            .await?;
        expect(page.locator(".guided-tour-card"))
            .to_have_css("background-color", "rgb(13, 17, 23)")
            .await?;
        Recording::new(&spec.session, "guided-tour-dark-portuguese")
            .finish(true)
            .await?;
        page.locator("button[aria-label='Fechar guia']")
            .click(None)
            .await?;
        expect(page.locator("dialog[data-guided-tour]"))
            .to_have_count(0)
            .await?;
        expect(guide).to_be_focused().await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

async fn expect_card_in_viewport(page: &Page, width: f64, height: f64) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let card = page
                .locator(".guided-tour-card")
                .bounding_box()
                .await?
                .context("tour card bounds")?;
            if card.x >= 0.0
                && card.y >= 0.0
                && card.x + card.width <= width
                && card.y + card.height <= height
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .context("tour card did not fit the viewport")??;
    expect(page.locator(".guided-tour-actions button:last-child"))
        .to_be_visible()
        .await?;
    expect(page.locator("dialog button[aria-label$='guide']"))
        .to_be_visible()
        .await?;
    Ok(())
}
