use std::sync::Arc;

use gtl_application::{diffs::View, viewer::RenderOptions};
use gtl_benchmarks::require;

use super::view_fixture;

#[derive(Debug, Clone)]
pub(super) struct ViewerRenderBenchmark {
    view: Arc<View>,
}

impl ViewerRenderBenchmark {
    pub(super) fn fixture_45k() -> Self {
        Self {
            view: Arc::new(view_fixture::large_view()),
        }
    }

    pub(super) fn render_raw_artifact(&self, options: RenderOptions) -> String {
        require(
            gtl_artifacts::build_html(&self.view, options, Some("dark")),
            "rendering the raw benchmark artifact",
        )
    }
}
