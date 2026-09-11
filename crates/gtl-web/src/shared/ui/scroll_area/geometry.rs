#[derive(Clone, Copy)]
pub(super) struct ScrollbarGeometry {
    pub thumb_size: f64,
    pub thumb_offset: f64,
    scroll_max: f64,
    travel: f64,
}

impl ScrollbarGeometry {
    pub fn new(viewport: f64, content: f64, position: f64) -> Self {
        let scroll_max = (content - viewport).max(0.0);
        let thumb_size = if content > 0.0 {
            (viewport * viewport / content).max(24.0).min(viewport)
        } else {
            viewport
        };
        let travel = viewport - thumb_size;
        let thumb_offset = if scroll_max > 0.0 {
            position.clamp(0.0, scroll_max) / scroll_max * travel
        } else {
            0.0
        };
        Self {
            thumb_size,
            thumb_offset,
            scroll_max,
            travel,
        }
    }

    pub fn page_delta(self, pointer_offset: f64, viewport: f64) -> f64 {
        if pointer_offset < self.thumb_offset {
            -viewport
        } else {
            viewport
        }
    }

    pub fn scroll_delta(self, pointer_delta: f64) -> f64 {
        if self.travel > 0.0 {
            pointer_delta / self.travel * self.scroll_max
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollbarGeometry;

    #[test]
    fn dragging_the_full_track_reaches_the_end_of_large_content() {
        let geometry = ScrollbarGeometry::new(300.0, 30_000.0, 0.0);
        assert!((geometry.thumb_size - 24.0).abs() < f64::EPSILON);
        assert!((geometry.scroll_delta(276.0) - 29_700.0).abs() < f64::EPSILON);
        let end = ScrollbarGeometry::new(300.0, 30_000.0, 29_700.0);
        assert!((end.thumb_offset - 276.0).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_or_short_content_has_no_thumb_travel() {
        for content in [0.0, 100.0, 300.0] {
            let geometry = ScrollbarGeometry::new(300.0, content, 40.0);
            assert!((geometry.scroll_delta(80.0) - 0.0).abs() < f64::EPSILON);
            assert!((geometry.thumb_offset - 0.0).abs() < f64::EPSILON);
        }
    }
}
