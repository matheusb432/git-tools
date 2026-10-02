const VIEWPORT_MARGIN_PX: f64 = 12.0;
const SPOTLIGHT_PADDING_PX: f64 = 6.0;
const CARD_GAP_PX: f64 = 12.0;
const CARD_WIDTH_PX_MAX: f64 = 360.0;
const CARD_HEIGHT_PX_MIN: f64 = 240.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GuidedTourRect {
    pub(super) left_px: f64,
    pub(super) top_px: f64,
    pub(super) width_px: f64,
    pub(super) height_px: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct GuidedTourViewport {
    pub(super) width_px: f64,
    pub(super) height_px: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CardEdge {
    Top(f64),
    Bottom(f64),
    Centered,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GuidedTourLayout {
    spotlight: Option<GuidedTourRect>,
    card_left_px: f64,
    card_width_px: f64,
    card_height_px_max: f64,
    card_edge: CardEdge,
}

impl Default for GuidedTourLayout {
    fn default() -> Self {
        guided_tour_layout(None, GuidedTourViewport::default())
    }
}

impl GuidedTourLayout {
    pub(super) fn spotlight_style(self) -> String {
        self.spotlight.map_or_else(
            || "left:50%;top:50%;width:0;height:0".to_owned(),
            |rect| {
                format!(
                    "left:{}px;top:{}px;width:{}px;height:{}px",
                    rect.left_px, rect.top_px, rect.width_px, rect.height_px
                )
            },
        )
    }

    pub(super) fn card_style(self) -> String {
        let horizontal = format!(
            "left:{}px;width:{}px;max-height:{}px",
            self.card_left_px, self.card_width_px, self.card_height_px_max
        );
        // Dioxus retains omitted inline properties across updates.
        match self.card_edge {
            CardEdge::Top(top) => format!("{horizontal};top:{top}px;bottom:auto;transform:none"),
            CardEdge::Bottom(bottom) => {
                format!("{horizontal};top:auto;bottom:{bottom}px;transform:none")
            }
            CardEdge::Centered => {
                format!("{horizontal};top:50%;bottom:auto;transform:translateY(-50%)")
            }
        }
    }
}

pub(super) fn guided_tour_layout(
    target: Option<GuidedTourRect>,
    viewport: GuidedTourViewport,
) -> GuidedTourLayout {
    let width = viewport.width_px.max(0.0);
    let height = viewport.height_px.max(0.0);
    let margin = VIEWPORT_MARGIN_PX.min(width / 2.0).min(height / 2.0);
    let card_width_px = (width - 2.0 * margin).clamp(0.0, CARD_WIDTH_PX_MAX);
    let card_height_px_max = (height - 2.0 * margin).max(0.0);
    let spotlight = target
        .filter(|target| target.width_px > 0.0 && target.height_px > 0.0)
        .map(|target| {
            let left = (target.left_px - SPOTLIGHT_PADDING_PX).clamp(0.0, width);
            let top = (target.top_px - SPOTLIGHT_PADDING_PX).clamp(0.0, height);
            let right = (target.left_px + target.width_px + SPOTLIGHT_PADDING_PX).clamp(0.0, width);
            let bottom =
                (target.top_px + target.height_px + SPOTLIGHT_PADDING_PX).clamp(0.0, height);
            GuidedTourRect {
                left_px: left,
                top_px: top,
                width_px: (right - left).max(0.0),
                height_px: (bottom - top).max(0.0),
            }
        })
        .filter(|target| target.width_px > 0.0 && target.height_px > 0.0);
    let Some(spotlight) = spotlight else {
        return GuidedTourLayout {
            spotlight: None,
            card_left_px: (width - card_width_px) / 2.0,
            card_width_px,
            card_height_px_max,
            card_edge: CardEdge::Centered,
        };
    };
    let card_left_px = (spotlight.left_px + spotlight.width_px / 2.0 - card_width_px / 2.0)
        .clamp(margin, (width - margin - card_width_px).max(margin));
    let below = height - spotlight.top_px - spotlight.height_px - CARD_GAP_PX - margin;
    let above = spotlight.top_px - CARD_GAP_PX - margin;
    let (card_edge, card_height_px_max) = if below >= CARD_HEIGHT_PX_MIN {
        (
            CardEdge::Top(spotlight.top_px + spotlight.height_px + CARD_GAP_PX),
            below,
        )
    } else if above >= CARD_HEIGHT_PX_MIN {
        (
            CardEdge::Bottom(height - spotlight.top_px + CARD_GAP_PX),
            above,
        )
    } else {
        (CardEdge::Bottom(margin), card_height_px_max)
    };
    GuidedTourLayout {
        spotlight: Some(spotlight),
        card_left_px,
        card_width_px,
        card_height_px_max,
        card_edge,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: GuidedTourViewport = GuidedTourViewport {
        width_px: 800.0,
        height_px: 600.0,
    };

    fn rect(left_px: f64, top_px: f64, width_px: f64, height_px: f64) -> GuidedTourRect {
        GuidedTourRect {
            left_px,
            top_px,
            width_px,
            height_px,
        }
    }

    #[test]
    fn unavailable_targets_center_the_card() {
        for target in [
            None,
            Some(rect(20.0, 30.0, 0.0, 0.0)),
            Some(rect(900.0, 30.0, 40.0, 40.0)),
        ] {
            let layout = guided_tour_layout(target, VIEWPORT);
            assert_eq!(layout.spotlight, None);
            assert_eq!(layout.card_edge, CardEdge::Centered);
            assert_eq!(layout.card_left_px, 220.0);
        }
    }

    #[test]
    fn cards_use_available_space_below_and_above_targets() {
        let top = guided_tour_layout(Some(rect(750.0, 10.0, 40.0, 40.0)), VIEWPORT);
        assert_eq!(top.spotlight, Some(rect(744.0, 4.0, 52.0, 52.0)));
        assert_eq!(top.card_edge, CardEdge::Top(68.0));
        assert_eq!(top.card_left_px, 428.0);
        let bottom = guided_tour_layout(Some(rect(20.0, 550.0, 40.0, 40.0)), VIEWPORT);
        assert_eq!(bottom.card_edge, CardEdge::Bottom(68.0));
    }

    #[test]
    fn small_viewports_clip_the_spotlight_and_keep_card_navigation_on_screen() {
        for (width, height) in [(320.0, 240.0), (844.0, 390.0), (20.0, 20.0), (0.0, 0.0)] {
            let viewport = GuidedTourViewport {
                width_px: width,
                height_px: height,
            };
            let layout = guided_tour_layout(
                Some(rect(-40.0, -40.0, width + 100.0, height + 100.0)),
                viewport,
            );
            assert_eq!(
                layout.spotlight,
                (width > 0.0 && height > 0.0).then_some(rect(0.0, 0.0, width, height))
            );
            assert!(layout.card_left_px >= 0.0);
            assert!(layout.card_left_px + layout.card_width_px <= width);
            assert!(layout.card_height_px_max <= height);
            assert!(layout.card_height_px_max >= 0.0);
        }
    }
}
