use std::sync::Arc;

use application::{
    diffs::View,
    viewer::{
        RenderOptions, Theme, ViewerDocument, ViewerSettings, ViewerTab, ViewerTabId,
        ViewerTabKind, ViewerTabState, ViewerView,
    },
};
use desktop::MaudViewerRenderer;

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
        preview::build_html(&self.view, RenderOptions::DEFAULT, Some("dark"))
    }
}
