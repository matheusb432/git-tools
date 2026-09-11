//! The `viewer/search_viewer_files` query: match changed-file paths for one active view.

use gtl_wire::viewer::{SearchViewerFiles, ViewerDiffFileId, ViewerFileSearchResult};

use super::{
    ViewerState,
    source::{self, ViewerSourceError},
};
use crate::{diffs::View, ports::UserSettingsReader};

/// Searches changed-file paths without exposing the diff view to the client.
#[cqrsy::query]
pub fn execute(
    query: &SearchViewerFiles,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
) -> Result<ViewerFileSearchResult, ViewerSourceError> {
    let snapshot = source::current(query.identity, state, settings)?;
    Ok(search(query, snapshot.view()))
}

fn search(query: &SearchViewerFiles, view: &View) -> ViewerFileSearchResult {
    let needle = query.query.to_lowercase();
    let files = view
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.path.to_string_lossy().to_lowercase().contains(&needle))
        .map(|(index, _)| ViewerDiffFileId::for_index(index))
        .collect();
    ViewerFileSearchResult {
        identity: query.identity,
        files,
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::DiffLineCount,
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId},
    };
    use gtl_wire::viewer::{
        SearchViewerFiles, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions,
        ViewerViewIdentity,
    };

    use super::*;
    use crate::{
        diffs::FileDiff,
        utils::{diffs::view, repository_relative_path},
        viewer::search_viewer_files,
    };

    fn identity() -> ViewerViewIdentity {
        ViewerViewIdentity {
            tab_id: ViewerTabId::try_new(7).unwrap(),
            range_generation: ViewerRangeGeneration::new(2),
            selection_generation: ViewerSelectionGeneration::new(3),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        }
    }

    #[test]
    fn file_search_is_case_insensitive_and_preserves_source_order() {
        let mut view = view();
        view.files = ["src/Render.rs", "docs/rendering.md", "src/model.rs"]
            .into_iter()
            .map(|path| FileDiff {
                path: repository_relative_path(path),
                added: DiffLineCount::default(),
                removed: DiffLineCount::default(),
                lines: crate::diffs::source_lines::DiffSourceLines::default(),
                full_lines: None,
            })
            .collect();

        let result = search_viewer_files::search(
            &SearchViewerFiles {
                identity: identity(),
                query: "RENDER".into(),
            },
            &view,
        );

        assert_eq!(
            result.files,
            [
                ViewerDiffFileId::for_index(0),
                ViewerDiffFileId::for_index(1)
            ]
        );
    }
}
