#[cfg(feature = "desktop")]
use std::collections::HashSet;

use dioxus::prelude::*;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{
    SearchViewerFiles, ViewerDiffFileId, ViewerFileSearchResult, ViewerViewIdentity,
};
use gtl_wire::viewer::{ViewerActiveView, ViewerFileSummary};

#[cfg(feature = "desktop")]
use crate::{entities::diffs::viewer_server, shared::viewer_client::ViewerClientError};

#[derive(Clone, PartialEq)]
pub(super) enum WorkspaceFileMatches {
    Ready(Vec<ViewerFileSummary>),
    #[cfg(feature = "desktop")]
    Loading,
    #[cfg(feature = "desktop")]
    Error(ViewerClientError),
}

impl WorkspaceFileMatches {
    pub(super) fn files(&self) -> &[ViewerFileSummary] {
        match self {
            Self::Ready(files) => files,
            #[cfg(feature = "desktop")]
            Self::Loading | Self::Error(_) => &[],
        }
    }
}

pub(super) fn use_workspace_file_matches(
    view: ReadSignal<ViewerActiveView>,
    query: ReadSignal<String>,
    server_owned: bool,
) -> Memo<WorkspaceFileMatches> {
    #[cfg(feature = "desktop")]
    let search = use_workspace_file_search(view, query, server_owned);
    #[cfg(not(feature = "desktop"))]
    let _ = server_owned;
    use_memo(move || {
        let query = query.read();
        let view = view.read();
        #[cfg(feature = "desktop")]
        if server_owned && !query.is_empty() {
            return server_file_matches(&search, &view, &query);
        }
        let normalized = query.to_lowercase();
        WorkspaceFileMatches::Ready(
            view.files
                .iter()
                .filter(|file| {
                    file.path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&normalized)
                })
                .cloned()
                .collect(),
        )
    })
}

#[cfg(feature = "desktop")]
fn server_file_matches(
    search: &WorkspaceFileSearch,
    view: &ViewerActiveView,
    query: &str,
) -> WorkspaceFileMatches {
    let search = search.read();
    let Some(Some(outcome)) = search.as_ref() else {
        return WorkspaceFileMatches::Loading;
    };
    if let Some(files) = outcome.files_for(view.identity, query) {
        return WorkspaceFileMatches::Ready(
            view.files
                .iter()
                .filter(|file| files.contains(&file.id))
                .cloned()
                .collect(),
        );
    }
    if let Some(error) = outcome.error_for(view.identity, query) {
        return WorkspaceFileMatches::Error(error.clone());
    }
    WorkspaceFileMatches::Loading
}

#[cfg(feature = "desktop")]
type WorkspaceFileSearch = Resource<Option<WorkspaceFileSearchOutcome>>;

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceFileSearchOutcome {
    identity: ViewerViewIdentity,
    query: String,
    result: Result<HashSet<ViewerDiffFileId>, ViewerClientError>,
}

#[cfg(feature = "desktop")]
impl WorkspaceFileSearchOutcome {
    fn files_for(
        &self,
        identity: ViewerViewIdentity,
        query: &str,
    ) -> Option<&HashSet<ViewerDiffFileId>> {
        if self.identity != identity || self.query != query {
            return None;
        }
        self.result.as_ref().ok()
    }

    fn error_for(&self, identity: ViewerViewIdentity, query: &str) -> Option<&ViewerClientError> {
        if self.identity != identity || self.query != query {
            return None;
        }
        self.result.as_ref().err()
    }
}

#[cfg(feature = "desktop")]
fn use_workspace_file_search(
    view: ReadSignal<ViewerActiveView>,
    file_filter: ReadSignal<String>,
    server_owned: bool,
) -> WorkspaceFileSearch {
    let identity = use_memo(move || view.read().identity);
    use_resource(move || {
        let identity = identity();
        let query = file_filter.read().clone();
        request_workspace_files(view, server_owned, identity, query)
    })
}

#[cfg(feature = "desktop")]
async fn request_workspace_files(
    view: ReadSignal<ViewerActiveView>,
    server_owned: bool,
    identity: ViewerViewIdentity,
    query: String,
) -> Option<WorkspaceFileSearchOutcome> {
    if !server_owned || query.is_empty() {
        return None;
    }
    dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
    let result = viewer_server::search_files(SearchViewerFiles {
        identity,
        query: query.clone(),
    })
    .await
    .and_then(|result| validate_file_search_result(&view.peek(), identity, result));
    Some(WorkspaceFileSearchOutcome {
        identity,
        query,
        result,
    })
}

#[cfg(feature = "desktop")]
fn validate_file_search_result(
    view: &ViewerActiveView,
    identity: ViewerViewIdentity,
    result: ViewerFileSearchResult,
) -> Result<HashSet<ViewerDiffFileId>, ViewerClientError> {
    if result.identity != identity {
        return Err(ViewerClientError::InvalidMessage);
    }
    let result_count = result.files.len();
    let files = result.files.into_iter().collect::<HashSet<_>>();
    if files.len() != result_count
        || files
            .iter()
            .any(|file_id| !view.files.iter().any(|file| &file.id == file_id))
    {
        return Err(ViewerClientError::InvalidMessage);
    }
    Ok(files)
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use std::collections::HashSet;

    use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration};
    use gtl_wire::viewer::{
        ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerRenderOptions,
        ViewerViewIdentity,
    };

    use super::WorkspaceFileSearchOutcome;
    use crate::test_support::{TestResult, viewer_tab_id};
    #[test]
    fn completed_search_applies_only_to_its_identity_and_query() -> TestResult {
        let identity = ViewerViewIdentity {
            tab_id: viewer_tab_id(4)?,
            range_generation: ViewerRangeGeneration::new(2),
            selection_generation: ViewerSelectionGeneration::new(1),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        };
        let file_id = ViewerDiffFileId::for_index(3);
        let outcome = WorkspaceFileSearchOutcome {
            identity,
            query: "src".to_owned(),
            result: Ok(HashSet::from([file_id.clone()])),
        };

        assert_eq!(
            outcome.files_for(identity, "src"),
            Some(&HashSet::from([file_id]))
        );
        assert_eq!(outcome.files_for(identity, "tests"), None);
        assert_eq!(
            outcome.files_for(
                ViewerViewIdentity {
                    selection_generation: ViewerSelectionGeneration::new(2),
                    ..identity
                },
                "src",
            ),
            None
        );
        Ok(())
    }
}
