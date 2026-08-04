use std::sync::Arc;

use gtl_application::{
    diffs::View,
    viewer::{
        RenderOptions, Theme, ViewerDocument, ViewerSettings, ViewerTab, ViewerTabId,
        ViewerTabKind, ViewerTabState, ViewerView,
    },
};
use gtl_benchmarks::require;
use gtl_desktop::MaudViewerRenderer;

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

    pub(super) fn fixture_115_files() -> Self {
        Self {
            view: Arc::new(many_file_view()),
        }
    }

    pub(super) fn render(&self, options: RenderOptions) -> String {
        let tab_id = require(ViewerTabId::try_new(1), "creating a benchmark tab id");
        let document = require(
            ViewerDocument::new(
                vec![ViewerTab::new(
                    tab_id,
                    "45k-line benchmark".into(),
                    ViewerTabKind::Snapshot,
                    ViewerTabState::Ready,
                )],
                Some(tab_id),
                Some(ViewerView::new(
                    tab_id,
                    Arc::clone(&self.view),
                    options,
                    ViewerTabKind::Snapshot,
                )),
                vec![],
                ViewerSettings::new(options, Theme::Dark),
            ),
            "building a benchmark viewer document",
        );
        require(
            MaudViewerRenderer.build_view(&document),
            "rendering the benchmark viewer",
        )
    }

    pub(super) fn render_raw(&self) -> String {
        require(
            gtl_preview::build_html(&self.view, RenderOptions::DEFAULT, Some("dark")),
            "rendering the raw benchmark artifact",
        )
    }

    pub(super) fn render_shell(&self, options: RenderOptions) -> String {
        require(
            gtl_preview::view_shell(
                &self.view,
                options,
                require(ViewerTabId::try_new(1), "creating a benchmark tab id"),
                1,
            ),
            "rendering the benchmark view shell",
        )
        .into_string()
    }

    pub(super) fn render_chunks(&self, options: RenderOptions) -> Vec<gtl_preview::ViewChunk> {
        require(
            gtl_preview::view_chunks(&self.view, options),
            "rendering the benchmark view chunks",
        )
        .into()
    }
}

fn many_file_view() -> View {
    const FILE_COUNT: usize = 115;
    const LINES_PER_FILE: usize = 79;
    let mut view = view_fixture::view_with_lines(LINES_PER_FILE);
    let template = view.files.remove(0);
    view.files = (0..FILE_COUNT)
        .map(|index| {
            let mut file = template.clone();
            file.path = format!("src/generated/file-{index:03}.rs");
            file
        })
        .collect();
    view.title = "115-file diff".into();
    view
}
