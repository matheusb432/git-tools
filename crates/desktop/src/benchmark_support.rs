//! Stable benchmark fixtures for the server-rendered viewer.

use std::sync::Arc;

use application::{
    diffs::{Cmd, FileDiff, Foot, LineOwners, View},
    viewer::{
        RenderOptions, Theme, ViewerDocument, ViewerSettings, ViewerTab, ViewerTabId,
        ViewerTabKind, ViewerTabState, ViewerView,
    },
};

use crate::render::MaudViewerRenderer;

const FIXTURE_LINE_COUNT: usize = 45_000;

/// Owns the deterministic 45,000-line viewer fixture used by render benchmarks.
#[derive(Debug, Clone)]
pub struct ViewerRenderBenchmark {
    view: Arc<View>,
}

impl ViewerRenderBenchmark {
    /// Builds the deterministic large-document fixture.
    ///
    /// # Examples
    ///
    /// ```
    /// use desktop::benchmark_support::ViewerRenderBenchmark;
    ///
    /// let fixture = ViewerRenderBenchmark::fixture_45k();
    /// ```
    #[must_use]
    pub fn fixture_45k() -> Self {
        let view = Arc::new(large_view());
        Self { view }
    }

    /// Renders the fixture with the selected display options.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::viewer::RenderOptions;
    /// use desktop::benchmark_support::ViewerRenderBenchmark;
    ///
    /// let fixture = ViewerRenderBenchmark::fixture_45k();
    /// let html = fixture.render(RenderOptions::DEFAULT);
    /// assert!(html.contains("diff-unified"));
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the benchmark's hard-coded positive tab identifier or document invariants are
    /// changed into an invalid fixture.
    #[must_use]
    pub fn render(&self, options: RenderOptions) -> String {
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
}

fn large_view() -> View {
    let mut lines = Vec::with_capacity(FIXTURE_LINE_COUNT);
    lines.push(format!(
        "@@ -1,{FIXTURE_LINE_COUNT} +1,{FIXTURE_LINE_COUNT} @@"
    ));
    lines.extend(
        (1..FIXTURE_LINE_COUNT)
            .map(|line| format!(" line {line:05}: deterministic benchmark payload")),
    );

    View {
        repo_name: "benchmark".into(),
        repo_root: "/fixtures/benchmark".into(),
        branch: "main".into(),
        upstream: "origin/main".into(),
        commits: vec![],
        files: vec![FileDiff {
            path: "src/large.rs".into(),
            added: 0,
            removed: 0,
            full_lines: Some(lines.clone()),
            lines,
            commits: vec![],
            owners: LineOwners::default(),
        }],
        title: "Large diff".into(),
        cmd: Cmd {
            lead: "git diff ".into(),
            range: "origin/main..HEAD".into(),
            trail: String::new(),
        },
        commits_label: "0 commits".into(),
        foot: Foot {
            cmd: "git diff origin/main..HEAD".into(),
            note: "benchmark fixture".into(),
        },
        theme: None,
    }
}

#[cfg(test)]
mod tests {
    use application::viewer::{DiffDensity, DiffLayout};

    use super::*;

    #[test]
    fn fixture_renders_both_benchmark_variants() {
        let fixture = ViewerRenderBenchmark::fixture_45k();

        let unified = fixture.render(RenderOptions::new(
            DiffLayout::Unified,
            DiffDensity::Compact,
        ));
        let split = fixture.render(RenderOptions::new(DiffLayout::Split, DiffDensity::Full));

        assert!(unified.contains("diff-unified diff-compact"));
        assert!(split.contains("diff-split diff-full"));
    }
}
