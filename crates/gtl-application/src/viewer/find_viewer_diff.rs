//! The `viewer/find_viewer_diff` query: navigate rendered diff-row text on the server.

use gtl_models::failure::{ErrorMeta, ViewerFailure};
use gtl_wire::viewer::{
    FindViewerDiff, ViewerDiffFileId, ViewerDiffSearchDirection, ViewerDiffSearchMatch,
    ViewerDiffSearchResult,
};

use super::{
    ViewerState,
    rows::{ViewerWorkCancellation, indexed_layout},
    source::{self, ViewerSourceError},
    viewer_diff_file_source,
};
use crate::{diffs::View, ports::UserSettingsReader};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum FindViewerDiffError {
    #[error(transparent)]
    #[meta(transparent)]
    Source(#[from] ViewerSourceError),
    #[error("viewer diff search row index exceeds u32")]
    #[meta(failure = ViewerFailure::SearchTooLarge)]
    RowIndexExhausted,
    #[error("viewer diff search match count exceeds u64")]
    #[meta(failure = ViewerFailure::SearchTooLarge)]
    MatchCountExhausted,
    #[error("viewer diff search was cancelled")]
    #[meta(private(Cancelled))]
    Cancelled,
}

/// Finds one logical rendered row and counts all matches for browser-like navigation.
#[cqrsy::query]
pub fn execute(
    query: &FindViewerDiff,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    cancellation: &ViewerWorkCancellation,
) -> Result<ViewerDiffSearchResult, FindViewerDiffError> {
    let snapshot = source::ready(query.identity, state, settings)?;
    find(query, snapshot.view(), cancellation)
}

fn find(
    query: &FindViewerDiff,
    view: &View,
    cancellation: &ViewerWorkCancellation,
) -> Result<ViewerDiffSearchResult, FindViewerDiffError> {
    if cancellation.is_cancelled() {
        return Err(FindViewerDiffError::Cancelled);
    }
    let needle = query.query.to_lowercase();
    if needle.is_empty() {
        return Ok(ViewerDiffSearchResult {
            identity: query.identity,
            total_matches: 0,
            active_match: None,
            wrapped: false,
        });
    }
    let anchor = query
        .anchor
        .as_ref()
        .and_then(|anchor| search_key(view, anchor));
    let mut search = SearchAccumulator::new(query.direction, anchor);

    for file_index in 0..view.files.len() {
        search_file(&mut search, query, view, file_index, &needle, cancellation)?;
    }

    Ok(search.finish(query.identity))
}

fn search_file(
    search: &mut SearchAccumulator,
    query: &FindViewerDiff,
    view: &View,
    file_index: usize,
    needle: &str,
    cancellation: &ViewerWorkCancellation,
) -> Result<(), FindViewerDiffError> {
    let file = ViewerDiffFileId::for_index(file_index);
    let Some(source) = viewer_diff_file_source(view, &file, query.identity.render_options.density)
    else {
        return Ok(());
    };
    let layout = indexed_layout(query.identity.render_options.layout);
    let index = source.lines.index();
    for row_index in 0..index.len(layout) {
        if cancellation.is_cancelled() {
            return Err(FindViewerDiffError::Cancelled);
        }
        let matches = index.row(layout, row_index).is_some_and(|row| {
            row.lines().any(|line| {
                let text = source.lines.line(line.source_index);
                let text = match line.kind {
                    gtl_parser::DiffRowKind::Meta | gtl_parser::DiffRowKind::Hunk => text,
                    gtl_parser::DiffRowKind::Context
                    | gtl_parser::DiffRowKind::Added
                    | gtl_parser::DiffRowKind::Removed => gtl_parser::diff_line_body(text),
                };
                contains_case_insensitive(text, needle)
            })
        });
        if matches {
            search.observe(file_index, row_index, file.clone())?;
        }
    }
    Ok(())
}

fn contains_case_insensitive(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(needle)
}

fn search_key(view: &View, found: &ViewerDiffSearchMatch) -> Option<(usize, u32)> {
    view.files
        .iter()
        .enumerate()
        .find(|(index, _)| ViewerDiffFileId::for_index(*index) == found.file)
        .map(|(index, _)| (index, found.row_index))
}

struct SearchAccumulator {
    direction: ViewerDiffSearchDirection,
    anchor: Option<(usize, u32)>,
    first: Option<ViewerDiffSearchMatch>,
    last: Option<ViewerDiffSearchMatch>,
    selected: Option<ViewerDiffSearchMatch>,
    found_in_direction: bool,
    total_matches: u64,
}

impl SearchAccumulator {
    const fn new(direction: ViewerDiffSearchDirection, anchor: Option<(usize, u32)>) -> Self {
        Self {
            direction,
            anchor,
            first: None,
            last: None,
            selected: None,
            found_in_direction: false,
            total_matches: 0,
        }
    }

    fn observe(
        &mut self,
        file_index: usize,
        row_index: usize,
        file: ViewerDiffFileId,
    ) -> Result<(), FindViewerDiffError> {
        let row_index =
            u32::try_from(row_index).map_err(|_| FindViewerDiffError::RowIndexExhausted)?;
        self.total_matches = self
            .total_matches
            .checked_add(1)
            .ok_or(FindViewerDiffError::MatchCountExhausted)?;
        let found = ViewerDiffSearchMatch { file, row_index };
        self.first.get_or_insert_with(|| found.clone());
        self.last = Some(found.clone());
        let Some(anchor) = self.anchor else {
            return Ok(());
        };
        let key = (file_index, row_index);
        match self.direction {
            ViewerDiffSearchDirection::Forward if key > anchor && !self.found_in_direction => {
                self.selected = Some(found);
                self.found_in_direction = true;
            }
            ViewerDiffSearchDirection::Backward if key < anchor => {
                self.selected = Some(found);
                self.found_in_direction = true;
            }
            ViewerDiffSearchDirection::Forward | ViewerDiffSearchDirection::Backward => {}
        }
        Ok(())
    }

    fn finish(self, identity: gtl_wire::viewer::ViewerViewIdentity) -> ViewerDiffSearchResult {
        let active_match = self.selected.or(match self.direction {
            ViewerDiffSearchDirection::Forward => self.first,
            ViewerDiffSearchDirection::Backward => self.last,
        });
        ViewerDiffSearchResult {
            identity,
            total_matches: self.total_matches,
            active_match,
            wrapped: self.anchor.is_some() && self.total_matches > 0 && !self.found_in_direction,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use gtl_models::{
        diffs::DiffLineCount,
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId},
    };
    use gtl_wire::viewer::{
        FindViewerDiff, ViewerDiffDensity, ViewerDiffLayout, ViewerDiffSearchDirection,
        ViewerRenderOptions, ViewerViewIdentity,
    };

    use super::*;
    use crate::{
        diffs::FileDiff,
        utils::{diffs::view, repository_relative_path},
        viewer::find_viewer_diff,
    };

    pub(crate) fn identity(layout: ViewerDiffLayout) -> ViewerViewIdentity {
        ViewerViewIdentity {
            tab_id: ViewerTabId::try_new(7).unwrap(),
            range_generation: ViewerRangeGeneration::new(2),
            selection_generation: ViewerSelectionGeneration::new(3),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout,
                density: ViewerDiffDensity::Compact,
            },
        }
    }

    fn search_view() -> View {
        let mut view = view();
        view.files = vec![
            FileDiff {
                path: repository_relative_path("src/first.rs"),
                added: DiffLineCount::new(1),
                removed: DiffLineCount::new(1),
                lines: vec![
                    "@@ -1 +1 @@".into(),
                    "-old needle".into(),
                    "+new value".into(),
                ]
                .into(),
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/second.rs"),
                added: DiffLineCount::new(1),
                removed: DiffLineCount::default(),
                lines: vec!["@@ -0,0 +1 @@".into(), "+second NEEDLE".into()].into(),
                full_lines: None,
            },
        ];
        view
    }

    #[test]
    fn cancelled_search_returns_no_partial_match_count() {
        let cancellation = ViewerWorkCancellation::default();
        cancellation.cancel();
        let result = find_viewer_diff::find(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Split),
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor: None,
            },
            &search_view(),
            &cancellation,
        );
        assert!(matches!(result, Err(FindViewerDiffError::Cancelled)));
    }

    #[test]
    fn forward_search_counts_matches_and_wraps_across_files() {
        let view = search_view();
        let identity = identity(ViewerDiffLayout::Unified);
        let first = find_viewer_diff::find(
            &FindViewerDiff {
                identity,
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor: None,
            },
            &view,
            &ViewerWorkCancellation::default(),
        )
        .unwrap();
        assert_eq!(first.total_matches, 2);
        assert_eq!(
            first.active_match.as_ref().unwrap().file,
            ViewerDiffFileId::for_index(0)
        );
        assert!(!first.wrapped);

        let wrapped = find_viewer_diff::find(
            &FindViewerDiff {
                identity,
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor: Some(ViewerDiffSearchMatch {
                    file: ViewerDiffFileId::for_index(1),
                    row_index: u32::MAX,
                }),
            },
            &view,
            &ViewerWorkCancellation::default(),
        )
        .unwrap();
        assert_eq!(wrapped.active_match, first.active_match);
        assert!(wrapped.wrapped);
    }

    #[test]
    fn split_search_counts_a_matching_pair_as_one_logical_row() {
        let view = search_view();
        let result = find_viewer_diff::find(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Split),
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor: None,
            },
            &view,
            &ViewerWorkCancellation::default(),
        )
        .unwrap();

        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn backward_search_starts_at_the_last_match() {
        let view = search_view();
        let result = find_viewer_diff::find(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Unified),
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Backward,
                anchor: None,
            },
            &view,
            &ViewerWorkCancellation::default(),
        )
        .unwrap();

        assert_eq!(
            result.active_match.unwrap().file,
            ViewerDiffFileId::for_index(1)
        );
    }
}
