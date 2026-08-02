use std::sync::Arc;

use gtl_application::{
    diffs::View,
    viewer::{
        RenderOptions, Theme, ViewerDocument, ViewerSettings, ViewerTab, ViewerTabId,
        ViewerTabKind, ViewerTabState, ViewerView,
    },
};
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
        let tab_id = ViewerTabId::try_new(1).expect("fixture tab id is positive");
        let document = ViewerDocument::new(
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
        )
        .expect("benchmark render options preserve document invariants");
        MaudViewerRenderer.build_view(&document)
    }

    pub(super) fn render_raw(&self) -> String {
        gtl_preview::build_html(&self.view, RenderOptions::DEFAULT, Some("dark"))
    }

    pub(super) fn render_shell(&self, options: RenderOptions) -> String {
        gtl_preview::view_shell(
            &self.view,
            options,
            ViewerTabId::try_new(1).expect("fixture tab id is positive"),
            1,
        )
        .into_string()
    }

    pub(super) fn render_chunks(&self, options: RenderOptions) -> Vec<gtl_preview::ViewChunk> {
        gtl_preview::view_chunks(&self.view, options).into()
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
