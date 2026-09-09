//! The `viewer/read_viewer_diff_text` query: copy bounded source ranges without rendering.

use gtl_wire::viewer::{ReadViewerDiffText, VIEWER_ROW_MAX_ENCODED_BYTES, ViewerDiffTextLine};

use super::{
    ViewerState,
    rows::indexed_layout,
    source::{self, ViewerSourceError},
    viewer_diff_file_source,
};
use crate::{diffs::View, ports::UserSettingsReader};

#[derive(Debug, thiserror::Error)]
pub enum ReadViewerDiffTextError {
    #[error(transparent)]
    Source(#[from] ViewerSourceError),
    #[error("the requested source range is unavailable")]
    Unavailable,
    #[error("the requested source text exceeds the response limit")]
    TooLarge,
}

#[cqrsy::query]
pub fn execute(
    request: &ReadViewerDiffText,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
) -> Result<Vec<ViewerDiffTextLine>, ReadViewerDiffTextError> {
    let snapshot = source::ready(request.identity, state, settings)?;
    read_lines(request, snapshot.view())
}

fn read_lines(
    request: &ReadViewerDiffText,
    view: &View,
) -> Result<Vec<ViewerDiffTextLine>, ReadViewerDiffTextError> {
    let source =
        viewer_diff_file_source(view, &request.file, request.identity.render_options.density)
            .ok_or(ReadViewerDiffTextError::Unavailable)?;
    let layout = indexed_layout(request.identity.render_options.layout);
    let index = source.lines.index();
    if request.row_range.end() as usize > index.len(layout) {
        return Err(ReadViewerDiffTextError::Unavailable);
    }
    let mut lines = Vec::new();
    let mut bytes = 0usize;
    for row in request.row_range.start()..request.row_range.end() {
        let Some(row) = index.row(layout, row as usize) else {
            return Err(ReadViewerDiffTextError::Unavailable);
        };
        let line = row.lines().find_map(|line| {
            let number = if request.old_side {
                line.old_line_number
            } else {
                line.new_line_number
            }?;
            Some((
                number.into_inner(),
                gtl_parser::diff_line_body(source.lines.line(line.source_index)),
            ))
        });
        if let Some((line_number, text)) = line {
            bytes = bytes.saturating_add(text.len()).saturating_add(16);
            if bytes > VIEWER_ROW_MAX_ENCODED_BYTES {
                return Err(ReadViewerDiffTextError::TooLarge);
            }
            lines.push(ViewerDiffTextLine {
                line_number,
                text: text.to_owned(),
            });
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::{ViewerDiffFileId, ViewerDiffLayout, ViewerRowRange};

    use super::*;

    #[test]
    fn copies_indexed_source_sides_without_rendering_or_loading_previous_rows() {
        let mut view = crate::utils::diffs::view();
        view.files.push(crate::diffs::FileDiff {
            path: crate::utils::repository_relative_path("src/main.rs"),
            added: gtl_models::diffs::DiffLineCount::new(1),
            removed: gtl_models::diffs::DiffLineCount::new(1),
            full_lines: None,
            lines: ["@@ -1,2 +1,2 @@", " keep", "-old", "+new"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
                .into(),
        });
        for (layout, start, count) in [
            (ViewerDiffLayout::Unified, 1, 3),
            (ViewerDiffLayout::Split, 1, 2),
        ] {
            let mut request = ReadViewerDiffText {
                identity: super::super::find_viewer_diff::tests::identity(layout),
                file: ViewerDiffFileId::for_index(0),
                row_range: ViewerRowRange::try_new(start, count).unwrap(),
                old_side: false,
            };
            assert_eq!(
                read_lines(&request, &view)
                    .unwrap()
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                ["keep", "new"]
            );
            request.old_side = true;
            assert_eq!(
                read_lines(&request, &view)
                    .unwrap()
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                ["keep", "old"]
            );
            request.row_range = ViewerRowRange::try_new(999, 1).unwrap();
            assert!(matches!(
                read_lines(&request, &view),
                Err(ReadViewerDiffTextError::Unavailable)
            ));
        }
    }
}
