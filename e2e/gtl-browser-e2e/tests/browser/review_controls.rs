use anyhow::{Context as _, Result, ensure};
use playwright_rs::{
    expect,
    protocol::{Page, Viewport},
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn tab_rail_scrolls_with_the_wheel_and_reveals_the_active_diff() -> Result<()> {
    let spec = support::Spec::start_web("scrolling-tab-rail").await?;
    let outcome = async {
        let page = &spec.session.page;
        page.set_viewport_size(Viewport {
            width: 390,
            height: 844,
        })
        .await?;
        open_story(page, "viewer-tab-rail/narrow-rail").await?;
        let rail = page.get_by_role(playwright_rs::protocol::AriaRole::Tablist, None);
        expect(rail.clone())
            .to_have_css("scrollbar-width", "none")
            .await?;
        expect(rail.clone())
            .to_have_attribute("data-overflow-right", "true")
            .await?;
        rail.hover(None).await?;
        page.mouse().wheel(0.0, 400.0).await?;
        expect(rail.clone())
            .to_have_attribute("data-overflow-left", "true")
            .await?;

        support::get_button(page, "Review last diff")
            .click(None)
            .await?;
        expect(page.locator("[role='tab'][aria-selected='true']"))
            .to_contain_text("Documentation edits")
            .await?;
        expect(rail.clone())
            .to_have_attribute("data-overflow-right", "false")
            .await?;
        let viewport = rail.bounding_box().await?.context("tab rail bounds")?;
        let active = page
            .locator(".viewer-tab[data-active='true']")
            .bounding_box()
            .await?
            .context("active tab bounds")?;
        ensure!(
            active.x >= viewport.x - 1.0
                && active.x + active.width <= viewport.x + viewport.width + 1.0,
            "active tab was not revealed inside the rail"
        );

        support::get_button(page, "Review first diff")
            .click(None)
            .await?;
        expect(rail)
            .to_have_attribute("data-overflow-left", "false")
            .await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

#[tokio::test(flavor = "multi_thread")]
async fn unpushed_status_keeps_review_buttons_in_place() -> Result<()> {
    let spec = support::Spec::start_web("review-action-dock").await?;
    let outcome = async {
        let page = &spec.session.page;
        open_story(page, "review-actions/interactive").await?;
        for width in [1400, 320] {
            page.set_viewport_size(Viewport { width, height: 844 })
                .await?;
            let toggle = page.locator("#preview-unpushed");
            toggle.check(None).await?;
            let status = page
                .locator(".review-action-dock")
                .get_by_role(playwright_rs::protocol::AriaRole::Status, None);
            expect(status.clone())
                .to_contain_text("This snapshot contains commits that have not been pushed.")
                .await?;
            let push = page.locator("#review-push-trigger");
            let close = page.locator("#review-close-trigger");
            let before_push = push.bounding_box().await?.context("push button bounds")?;
            let before_close = close.bounding_box().await?.context("close button bounds")?;
            ensure!(
                before_push.x >= 0.0 && before_close.x + before_close.width <= f64::from(width),
                "review buttons overflowed the viewport"
            );
            toggle.uncheck(None).await?;
            expect(status).to_have_text("").await?;
            ensure!(
                push.bounding_box().await? == Some(before_push)
                    && close.bounding_box().await? == Some(before_close),
                "review buttons moved when the unpushed status changed"
            );
        }
        let push = page.locator("#review-push-trigger");
        page.locator("#review-actions-hide").click(None).await?;
        expect(push.clone()).to_be_hidden().await?;
        page.locator("#review-actions-show").click(None).await?;
        expect(push).to_be_visible().await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

async fn open_story(page: &Page, story: &str) -> Result<()> {
    let base = std::env::var("GTL_COMPONENT_PREVIEW_URL").context("component preview URL")?;
    page.goto(&format!("{base}/render/{story}"), None).await?;
    expect(page.locator("[data-dx-story-ready='true']"))
        .to_be_visible()
        .await?;
    Ok(())
}
