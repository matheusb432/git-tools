use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const DISTANCE_CSS_PIXELS: u32 = 160;
pub const STEP_CSS_PIXELS: u32 = 8;
pub const TRAVERSALS: u32 = 10;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ScrollProtocol {
    pub distance_css_pixels: u32,
    pub step_css_pixels: u32,
    pub traversals: u32,
}

impl ScrollProtocol {
    pub const fn fixed() -> Self {
        Self {
            distance_css_pixels: DISTANCE_CSS_PIXELS,
            step_css_pixels: STEP_CSS_PIXELS,
            traversals: TRAVERSALS,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BrowserScrollSample {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub frame_timestamps_ms: Vec<f64>,
    #[serde(default)]
    pub scroll_height_css_pixels: u64,
    #[serde(default)]
    pub client_height_css_pixels: u64,
    #[serde(default)]
    pub final_scroll_top_css_pixels: f64,
    #[serde(default)]
    pub inner_width_css_pixels: u32,
    #[serde(default)]
    pub inner_height_css_pixels: u32,
}

#[derive(Debug, Serialize)]
pub struct ScrollSample {
    pub panel: String,
    pub distance_css_pixels: u32,
    pub step_css_pixels: u32,
    pub traversals: u32,
    pub total_distance_css_pixels: u64,
    pub scroll_height_css_pixels: u64,
    pub client_height_css_pixels: u64,
    pub final_scroll_top_css_pixels: f64,
    pub inner_width_css_pixels: u32,
    pub inner_height_css_pixels: u32,
    pub frame_count: usize,
    pub frame_gap_count: usize,
    pub total_duration_ms: f64,
    pub mean_frame_gap_ms: f64,
    pub p50_frame_gap_ms: f64,
    pub p95_frame_gap_ms: f64,
    pub p99_frame_gap_ms: f64,
    pub maximum_frame_gap_ms: f64,
    pub frames_exceeding_33_ms: usize,
    pub frame_gaps_ms: Vec<f64>,
}

pub fn summarize(
    panel: &str,
    protocol: ScrollProtocol,
    raw: &BrowserScrollSample,
) -> Result<ScrollSample> {
    ensure!(
        raw.error.is_none(),
        "{panel} scroll script rejected the journey: {}",
        raw.error.as_deref().unwrap_or("unknown browser error")
    );
    ensure!(
        raw.frame_timestamps_ms.len() >= 2,
        "{panel} scroll returned fewer than two animation frames"
    );
    ensure!(
        raw.scroll_height_css_pixels
            >= raw.client_height_css_pixels + u64::from(protocol.distance_css_pixels),
        "{panel} has only {} CSS pixels of scroll range; {} are required",
        raw.scroll_height_css_pixels
            .saturating_sub(raw.client_height_css_pixels),
        protocol.distance_css_pixels
    );

    let mut frame_gaps_ms = Vec::with_capacity(raw.frame_timestamps_ms.len() - 1);
    for timestamps in raw.frame_timestamps_ms.windows(2) {
        let gap = timestamps[1] - timestamps[0];
        ensure!(
            gap.is_finite() && gap >= 0.0,
            "{panel} returned an invalid animation-frame gap: {gap}"
        );
        frame_gaps_ms.push(gap);
    }
    let total_duration_ms = raw
        .frame_timestamps_ms
        .last()
        .context("scroll sample has no final frame")?
        - raw
            .frame_timestamps_ms
            .first()
            .context("scroll sample has no first frame")?;
    let mut sorted_gaps = frame_gaps_ms.clone();
    sorted_gaps.sort_by(f64::total_cmp);
    let maximum_frame_gap_ms = *sorted_gaps
        .last()
        .context("scroll sample has no frame gaps")?;
    let frame_count = raw.frame_timestamps_ms.len();
    let frame_gap_count = frame_gaps_ms.len();

    Ok(ScrollSample {
        panel: panel.to_owned(),
        distance_css_pixels: protocol.distance_css_pixels,
        step_css_pixels: protocol.step_css_pixels,
        traversals: protocol.traversals,
        total_distance_css_pixels: u64::from(protocol.distance_css_pixels)
            * u64::from(protocol.traversals),
        scroll_height_css_pixels: raw.scroll_height_css_pixels,
        client_height_css_pixels: raw.client_height_css_pixels,
        final_scroll_top_css_pixels: raw.final_scroll_top_css_pixels,
        inner_width_css_pixels: raw.inner_width_css_pixels,
        inner_height_css_pixels: raw.inner_height_css_pixels,
        frame_count,
        frame_gap_count,
        total_duration_ms,
        mean_frame_gap_ms: mean(&frame_gaps_ms),
        p50_frame_gap_ms: nearest_rank(&sorted_gaps, 50),
        p95_frame_gap_ms: nearest_rank(&sorted_gaps, 95),
        p99_frame_gap_ms: nearest_rank(&sorted_gaps, 99),
        maximum_frame_gap_ms,
        frames_exceeding_33_ms: frame_gaps_ms.iter().filter(|gap| **gap > 33.0).count(),
        frame_gaps_ms,
    })
}

#[allow(clippy::cast_precision_loss)]
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn nearest_rank(sorted_values: &[f64], percentile: usize) -> f64 {
    let rank = sorted_values.len().saturating_mul(percentile).div_ceil(100);
    sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_protocol_collects_200_frame_gaps() {
        let protocol = ScrollProtocol::fixed();
        let frame_gap_count =
            protocol.distance_css_pixels / protocol.step_css_pixels * protocol.traversals;

        assert_eq!(frame_gap_count, 200);
    }

    #[test]
    fn frame_metrics_preserve_gaps_and_count_slow_frames() {
        let sample = summarize(
            "changed-files",
            ScrollProtocol::fixed(),
            &BrowserScrollSample {
                error: None,
                frame_timestamps_ms: vec![0.0, 16.0, 33.0, 68.0, 88.0],
                scroll_height_css_pixels: 800,
                client_height_css_pixels: 500,
                final_scroll_top_css_pixels: 0.0,
                inner_width_css_pixels: 1_200,
                inner_height_css_pixels: 700,
            },
        )
        .expect("summarize valid frame timestamps");

        assert_eq!(sample.frame_gaps_ms, [16.0, 17.0, 35.0, 20.0]);
        assert!((sample.total_duration_ms - 88.0).abs() < f64::EPSILON);
        assert!((sample.p50_frame_gap_ms - 17.0).abs() < f64::EPSILON);
        assert!((sample.p95_frame_gap_ms - 35.0).abs() < f64::EPSILON);
        assert!((sample.maximum_frame_gap_ms - 35.0).abs() < f64::EPSILON);
        assert_eq!(sample.frames_exceeding_33_ms, 1);
    }

    #[test]
    fn frame_metrics_reject_a_panel_without_the_fixed_scroll_range() {
        let error = summarize(
            "commits",
            ScrollProtocol::fixed(),
            &BrowserScrollSample {
                error: None,
                frame_timestamps_ms: vec![0.0, 16.0],
                scroll_height_css_pixels: 620,
                client_height_css_pixels: 500,
                final_scroll_top_css_pixels: 0.0,
                inner_width_css_pixels: 1_200,
                inner_height_css_pixels: 700,
            },
        )
        .expect_err("reject insufficient scroll range");

        assert!(error.to_string().contains("only 120 CSS pixels"));
    }
}
