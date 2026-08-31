//! The `viewer/find_viewer_diff` query: navigate rendered diff-row text on the server.

use gtl_wire::viewer::{
    FindViewerDiff, ViewerDiffFileId, ViewerDiffSearchDirection, ViewerDiffSearchMatch,
    ViewerDiffSearchResult, ViewerRows, ViewerSplitRow, ViewerUnifiedRow,
};

use super::{rows::parse_viewer_diff_file, viewer_diff_file_source};
use crate::diffs::View;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FindViewerDiffError {
    #[error("viewer diff search row index exceeds u32")]
    RowIndexExhausted,
    #[error("viewer diff search match count exceeds u64")]
    MatchCountExhausted,
}

/// Finds one logical rendered row and counts all matches for browser-like navigation.
#[cqrsy::query]
pub fn execute(
    query: &FindViewerDiff,
    view: &View,
) -> Result<ViewerDiffSearchResult, FindViewerDiffError> {
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
        search_file(&mut search, query, view, file_index, &needle)?;
    }

    Ok(search.finish(query.identity))
}

fn search_file(
    search: &mut SearchAccumulator,
    query: &FindViewerDiff,
    view: &View,
    file_index: usize,
    needle: &str,
) -> Result<(), FindViewerDiffError> {
    let file = ViewerDiffFileId::for_index(file_index);
    let Some(source) = viewer_diff_file_source(view, &file, query.identity.render_options.density)
    else {
        return Ok(());
    };
    let parsed = parse_viewer_diff_file(
        source.path,
        source.lines,
        query.identity.render_options.layout,
    );
    match parsed.file.rows {
        ViewerRows::Unified(rows) => {
            observe_matching_rows(search, file_index, &file, &rows, |row| {
                unified_row_matches(row, needle)
            })?;
        }
        ViewerRows::Split(rows) => {
            observe_matching_rows(search, file_index, &file, &rows, |row| {
                split_row_matches(row, needle)
            })?;
        }
    }
    Ok(())
}

fn observe_matching_rows<Row>(
    search: &mut SearchAccumulator,
    file_index: usize,
    file: &ViewerDiffFileId,
    rows: &[Row],
    matches: impl Fn(&Row) -> bool,
) -> Result<(), FindViewerDiffError> {
    for row_index in rows
        .iter()
        .enumerate()
        .filter(|(_, row)| matches(row))
        .map(|(row_index, _)| row_index)
    {
        search.observe(file_index, row_index, file.clone())?;
    }
    Ok(())
}

fn unified_row_matches(row: &ViewerUnifiedRow, needle: &str) -> bool {
    let text = match row {
        ViewerUnifiedRow::Meta(text) | ViewerUnifiedRow::Hunk(text) => text,
        ViewerUnifiedRow::Context(row)
        | ViewerUnifiedRow::Added(row)
        | ViewerUnifiedRow::Removed(row) => &row.code.text,
    };
    contains_case_insensitive(text, needle)
}

fn split_row_matches(row: &ViewerSplitRow, needle: &str) -> bool {
    match row {
        ViewerSplitRow::Meta(text) | ViewerSplitRow::Hunk(text) => {
            contains_case_insensitive(text, needle)
        }
        ViewerSplitRow::Context { code, .. } => contains_case_insensitive(&code.text, needle),
        ViewerSplitRow::Pair { old, new } => {
            old.as_ref()
                .is_some_and(|cell| contains_case_insensitive(&cell.code.text, needle))
                || new
                    .as_ref()
                    .is_some_and(|cell| contains_case_insensitive(&cell.code.text, needle))
        }
    }
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
    };

    pub(crate) fn identity(layout: ViewerDiffLayout) -> ViewerViewIdentity {
        ViewerViewIdentity {
            tab_id: ViewerTabId::try_new(7).unwrap(),
            range_generation: ViewerRangeGeneration::new(2),
            selection_generation: ViewerSelectionGeneration::new(3),
            render_options: ViewerRenderOptions {
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
                ],
                full_lines: None,
            },
            FileDiff {
                path: repository_relative_path("src/second.rs"),
                added: DiffLineCount::new(1),
                removed: DiffLineCount::default(),
                lines: vec!["@@ -0,0 +1 @@".into(), "+second NEEDLE".into()],
                full_lines: None,
            },
        ];
        view
    }

    #[test]
    fn forward_search_counts_matches_and_wraps_across_files() {
        let view = search_view();
        let identity = identity(ViewerDiffLayout::Unified);
        let first = execute(
            &FindViewerDiff {
                identity,
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor: None,
            },
            &view,
        )
        .unwrap();
        assert_eq!(first.total_matches, 2);
        assert_eq!(
            first.active_match.as_ref().unwrap().file,
            ViewerDiffFileId::for_index(0)
        );
        assert!(!first.wrapped);

        let wrapped = execute(
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
        )
        .unwrap();
        assert_eq!(wrapped.active_match, first.active_match);
        assert!(wrapped.wrapped);
    }

    #[test]
    fn split_search_counts_a_matching_pair_as_one_logical_row() {
        let view = search_view();
        let result = execute(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Split),
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Forward,
                anchor: None,
            },
            &view,
        )
        .unwrap();

        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn backward_search_starts_at_the_last_match() {
        let view = search_view();
        let result = execute(
            &FindViewerDiff {
                identity: identity(ViewerDiffLayout::Unified),
                query: "needle".into(),
                direction: ViewerDiffSearchDirection::Backward,
                anchor: None,
            },
            &view,
        )
        .unwrap();

        assert_eq!(
            result.active_match.unwrap().file,
            ViewerDiffFileId::for_index(1)
        );
    }
}
