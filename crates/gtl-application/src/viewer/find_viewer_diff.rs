//! The `viewer/find_viewer_diff` query: navigate rendered diff-row text on the server.

use std::collections::HashSet;

use gtl_models::failure::{ErrorMeta, Failure, ViewerFailure};
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
    #[error("viewer diff search files are unknown or repeated")]
    #[meta(failure = Failure::InvalidRequest { field: "files".to_owned() })]
    InvalidFiles,
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
    validate_files(query, view)?;
    let needle = query.query.to_lowercase();
    if needle.is_empty() {
        return Ok(ViewerDiffSearchResult {
            identity: query.identity,
            total_matches: 0,
            active_match: None,
            wrapped: false,
            matched_files: Vec::new(),
        });
    }
    let anchor = query.anchor.as_ref().and_then(|anchor| {
        query
            .files
            .iter()
            .position(|file| file == &anchor.file)
            .map(|position| (position, anchor.row_index))
    });
    let mut search = SearchAccumulator::new(query.direction, anchor);

    for (position, file) in query.files.iter().enumerate() {
        search_file(
            &mut search,
            query,
            view,
            position,
            file,
            &needle,
            cancellation,
        )?;
    }

    Ok(search.finish(query.identity))
}

fn validate_files(query: &FindViewerDiff, view: &View) -> Result<(), FindViewerDiffError> {
    let density = query.identity.render_options.density;
    let mut seen = HashSet::with_capacity(query.files.len());
    for file in &query.files {
        if !seen.insert(file) || viewer_diff_file_source(view, file, density).is_none() {
            return Err(FindViewerDiffError::InvalidFiles);
        }
    }
    Ok(())
}

fn search_file(
    search: &mut SearchAccumulator,
    query: &FindViewerDiff,
    view: &View,
    position: usize,
    file: &ViewerDiffFileId,
    needle: &str,
    cancellation: &ViewerWorkCancellation,
) -> Result<(), FindViewerDiffError> {
    let Some(source) = viewer_diff_file_source(view, file, query.identity.render_options.density)
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
            search.observe(position, row_index, file.clone())?;
        }
    }
    Ok(())
}

fn contains_case_insensitive(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(needle)
}

struct SearchAccumulator {
    direction: ViewerDiffSearchDirection,
    anchor: Option<(usize, u32)>,
    first: Option<ViewerDiffSearchMatch>,
    last: Option<ViewerDiffSearchMatch>,
    selected: Option<ViewerDiffSearchMatch>,
    found_in_direction: bool,
    total_matches: u64,
    matched_files: Vec<ViewerDiffFileId>,
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
            matched_files: Vec::new(),
        }
    }

    fn observe(
        &mut self,
        position: usize,
        row_index: usize,
        file: ViewerDiffFileId,
    ) -> Result<(), FindViewerDiffError> {
        let row_index =
            u32::try_from(row_index).map_err(|_| FindViewerDiffError::RowIndexExhausted)?;
        self.total_matches = self
            .total_matches
            .checked_add(1)
            .ok_or(FindViewerDiffError::MatchCountExhausted)?;
        if self.matched_files.last() != Some(&file) {
            self.matched_files.push(file.clone());
        }
        let found = ViewerDiffSearchMatch { file, row_index };
        self.first.get_or_insert_with(|| found.clone());
        self.last = Some(found.clone());
        let Some(anchor) = self.anchor else {
            return Ok(());
        };
        let key = (position, row_index);
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
            matched_files: self.matched_files,
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

    fn both_files() -> Vec<ViewerDiffFileId> {
        vec![
            ViewerDiffFileId::for_index(0),
            ViewerDiffFileId::for_index(1),
        ]
    }

    fn find_needle(
        files: Vec<ViewerDiffFileId>,
        anchor: Option<ViewerDiffSearchMatch>,
    ) -> Result<ViewerDiffSearchResult, FindViewerDiffError> {
        find_viewer_diff::find(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Unified),
                files,
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor,
            },
            &search_view(),
            &ViewerWorkCancellation::default(),
        )
    }

    #[test]
    fn search_follows_the_requested_file_order_and_reports_matched_files() {
        let reversed = vec![
            ViewerDiffFileId::for_index(1),
            ViewerDiffFileId::for_index(0),
        ];

        let first = find_needle(reversed.clone(), None).unwrap();
        let next = find_needle(reversed.clone(), first.active_match.clone()).unwrap();

        assert_eq!(first.matched_files, reversed);
        assert_eq!(
            first.active_match.unwrap().file,
            ViewerDiffFileId::for_index(1)
        );
        assert_eq!(
            next.active_match.unwrap().file,
            ViewerDiffFileId::for_index(0)
        );
        assert!(!next.wrapped);
    }

    #[test]
    fn search_counts_only_requested_files() {
        let result = find_needle(vec![ViewerDiffFileId::for_index(1)], None).unwrap();

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.matched_files, [ViewerDiffFileId::for_index(1)]);
    }

    #[test]
    fn unknown_or_repeated_files_are_rejected() {
        for files in [
            vec![ViewerDiffFileId::for_index(2)],
            vec![
                ViewerDiffFileId::for_index(0),
                ViewerDiffFileId::for_index(0),
            ],
        ] {
            assert!(matches!(
                find_needle(files, None),
                Err(FindViewerDiffError::InvalidFiles)
            ));
        }
    }

    #[test]
    fn cancelled_search_returns_no_partial_match_count() {
        let cancellation = ViewerWorkCancellation::default();
        cancellation.cancel();
        let result = find_viewer_diff::find(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Split),
                files: both_files(),
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
                files: both_files(),
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
                files: both_files(),
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
                files: both_files(),
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
                files: both_files(),
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
