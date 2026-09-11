use dioxus::prelude::*;
use gtl_wire::viewer::{
    FindViewerDiff, VIEWER_SEARCH_QUERY_MAX_BYTES, ViewerDiffSearchDirection,
    ViewerDiffSearchMatch, ViewerDiffSearchResult, ViewerViewIdentity,
};

use super::search_bar::{DiffSearchBar, DiffSearchScope};
use crate::{
    entities::diffs::{ClientDiffWorkspace, viewer_server},
    shared::{browser, viewer_client::ViewerClientError},
};

const FIND_INPUT_ID: &str = "viewer-diff-find-input";
const FIND_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);

#[derive(Clone, Copy)]
struct DiffFindController {
    query: ReadSignal<String>,
    state: ReadSignal<DiffFindState>,
    update_query: Callback<String>,
    navigate: Callback<ViewerDiffSearchDirection>,
    close: Callback<()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffFindRequest {
    identity: ViewerViewIdentity,
    query: String,
    direction: ViewerDiffSearchDirection,
    anchor: Option<ViewerDiffSearchMatch>,
    debounce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffFindState {
    Idle,
    QueryTooLong,
    Loading,
    Ready(ViewerDiffSearchResult),
    Error(ViewerClientError),
}

impl DiffFindState {
    fn active_match(&self) -> Option<ViewerDiffSearchMatch> {
        match self {
            Self::Ready(result) => result.active_match.clone(),
            Self::Idle | Self::QueryTooLong | Self::Loading | Self::Error(_) => None,
        }
    }

    fn navigation_enabled(&self) -> bool {
        matches!(self, Self::Ready(result) if result.total_matches > 0)
    }

    fn message(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::QueryTooLong => {
                format!("Search is limited to {VIEWER_SEARCH_QUERY_MAX_BYTES} UTF-8 bytes.")
            }
            Self::Loading => "Searching\u{2026}".into(),
            Self::Ready(result) if result.total_matches == 0 => "No matches".into(),
            Self::Ready(result) => format!(
                "{} {}{}",
                result.total_matches,
                match_count_label(result.total_matches),
                wrapped_label(result.wrapped),
            ),
            Self::Error(error) => error.message().to_owned(),
        }
    }
}

const fn match_count_label(count: u64) -> &'static str {
    if count == 1 { "match" } else { "matches" }
}

const fn wrapped_label(wrapped: bool) -> &'static str {
    if wrapped { " \u{00b7} wrapped" } else { "" }
}

fn use_diff_find(
    mut open: Signal<bool>,
    identity: ViewerViewIdentity,
    workspace: ReadStore<ClientDiffWorkspace>,
    mut target: Signal<Option<super::DiffSearchTarget>>,
) -> DiffFindController {
    let mut query = use_signal(String::new);
    let state = use_signal(|| DiffFindState::Idle);
    let search = use_action(move |next: DiffFindRequest| async move {
        execute_find_request(next, state, workspace, target).await;
        Ok::<(), std::convert::Infallible>(())
    });

    use_effect(move || {
        if open() {
            browser::focus_element(FIND_INPUT_ID.to_owned());
            queue_initial_search(search, state, identity, query.peek().clone());
        } else {
            let mut search = search;
            search.reset();
            target.set(None);
            browser::clear_diff_search_match();
        }
    });
    use_effect(use_reactive((&identity,), move |(identity,)| {
        browser::clear_diff_search_match();
        if *open.peek() {
            queue_initial_search(search, state, identity, query.peek().clone());
        }
    }));
    use_drop(browser::clear_diff_search_match);

    let update_query = use_callback(move |value: String| {
        query.set(value.clone());
        target.set(None);
        browser::clear_diff_search_match();
        queue_initial_search(search, state, identity, value);
    });
    let navigate = use_callback(move |direction| {
        queue_navigation(search, state, identity, query.peek().clone(), direction);
    });
    let close = use_callback(move |()| {
        open.set(false);
        target.set(None);
        browser::clear_diff_search_match();
        browser::focus_element("workspace-heading".to_owned());
    });

    DiffFindController {
        query: query.into(),
        state: state.into(),
        update_query,
        navigate,
        close,
    }
}

#[component]
pub(super) fn DiffFindBar(
    open: Signal<bool>,
    identity: ViewerViewIdentity,
    workspace: ReadStore<ClientDiffWorkspace>,
    target: Signal<Option<super::DiffSearchTarget>>,
) -> Element {
    let find = use_diff_find(open, identity, workspace, target);

    if !open() {
        return rsx! {};
    }
    let current_state = find.state.read().clone();
    let status_message = current_state.message();

    rsx! {
        DiffSearchBar {
            input_id: FIND_INPUT_ID,
            scope: DiffSearchScope::AllFiles,
            query: (find.query)(),
            status_message,
            navigation_enabled: current_state.navigation_enabled(),
            maxlength: VIEWER_SEARCH_QUERY_MAX_BYTES.to_string(),
            onquerychange: move |value| find.update_query.call(value),
            onprevious: move |()| find.navigate.call(ViewerDiffSearchDirection::Backward),
            onnext: move |()| find.navigate.call(ViewerDiffSearchDirection::Forward),
            onclose: move |()| find.close.call(()),
        }
    }
}

fn queue_initial_search(
    mut search: Action<(DiffFindRequest,), ()>,
    mut state: Signal<DiffFindState>,
    identity: ViewerViewIdentity,
    query: String,
) {
    if query.is_empty() {
        search.reset();
        state.set(DiffFindState::Idle);
        return;
    }
    if query.len() > VIEWER_SEARCH_QUERY_MAX_BYTES {
        search.reset();
        state.set(DiffFindState::QueryTooLong);
        return;
    }
    queue_search(
        search,
        state,
        identity,
        query,
        ViewerDiffSearchDirection::Forward,
        None,
        true,
    );
}

fn queue_navigation(
    search: Action<(DiffFindRequest,), ()>,
    state: Signal<DiffFindState>,
    identity: ViewerViewIdentity,
    query: String,
    direction: ViewerDiffSearchDirection,
) {
    if query.is_empty() || query.len() > VIEWER_SEARCH_QUERY_MAX_BYTES {
        return;
    }
    let anchor = state.peek().active_match();
    queue_search(search, state, identity, query, direction, anchor, false);
}

fn queue_search(
    mut search: Action<(DiffFindRequest,), ()>,
    mut state: Signal<DiffFindState>,
    identity: ViewerViewIdentity,
    query: String,
    direction: ViewerDiffSearchDirection,
    anchor: Option<ViewerDiffSearchMatch>,
    debounce: bool,
) {
    state.set(DiffFindState::Loading);
    search.call(DiffFindRequest {
        identity,
        query,
        direction,
        anchor,
        debounce,
    });
}

async fn execute_find_request(
    next: DiffFindRequest,
    mut state: Signal<DiffFindState>,
    workspace: ReadStore<ClientDiffWorkspace>,
    mut target: Signal<Option<super::DiffSearchTarget>>,
) {
    if next.debounce {
        dioxus_sdk_time::sleep(FIND_DEBOUNCE).await;
    }
    let result = viewer_server::find_diff(FindViewerDiff {
        identity: next.identity,
        query: next.query.clone(),
        direction: next.direction,
        anchor: next.anchor.clone(),
    })
    .await
    .and_then(|result| validate_diff_search_result(&workspace.peek(), next.identity, result));
    match result {
        Ok((result, row_target)) => {
            target.set(row_target.map(|(file, row)| super::DiffSearchTarget {
                identity: next.identity,
                file,
                row,
            }));
            if result.active_match.is_none() {
                browser::clear_diff_search_match();
            }
            state.set(DiffFindState::Ready(result));
        }
        Err(error) => state.set(DiffFindState::Error(error)),
    }
}

fn validate_diff_search_result(
    workspace: &ClientDiffWorkspace,
    identity: ViewerViewIdentity,
    result: ViewerDiffSearchResult,
) -> Result<(ViewerDiffSearchResult, Option<(usize, usize)>), ViewerClientError> {
    if workspace.identity != identity || result.identity != identity {
        return Err(ViewerClientError::Internal);
    }
    let result_shape_is_valid = (result.total_matches == 0) == result.active_match.is_none();
    if !result_shape_is_valid {
        return Err(ViewerClientError::Internal);
    }
    let row_target = result
        .active_match
        .as_ref()
        .map(|found| diff_search_match_target(workspace, found))
        .transpose()?;
    Ok((result, row_target))
}

fn diff_search_match_target(
    workspace: &ClientDiffWorkspace,
    found: &ViewerDiffSearchMatch,
) -> Result<(usize, usize), ViewerClientError> {
    let file_index = workspace
        .files
        .iter()
        .position(|file| file.summary.id == found.file)
        .ok_or(ViewerClientError::Internal)?;
    let row_count = workspace.files[file_index].summary.row_count;
    let row_index = usize::try_from(found.row_index).map_err(|_| ViewerClientError::Internal)?;
    if row_index >= row_count {
        return Err(ViewerClientError::Internal);
    }
    Ok((file_index, row_index))
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::DiffLineCount,
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
    };
    use gtl_wire::viewer::{
        ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary,
        ViewerRenderOptions, ViewerSplitRow, ViewerUnifiedRow,
    };

    use super::*;
    use crate::{
        entities::diffs::{ClientDiffFile, ClientDiffFileState, ClientDiffRows},
        test_support::{TestResult, absolute_file_path, repository_relative_path, viewer_tab_id},
    };

    fn identity(layout: ViewerDiffLayout) -> TestResult<ViewerViewIdentity> {
        Ok(ViewerViewIdentity {
            tab_id: viewer_tab_id(7)?,
            range_generation: ViewerRangeGeneration::new(2),
            selection_generation: ViewerSelectionGeneration::new(3),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout,
                density: ViewerDiffDensity::Compact,
            },
        })
    }

    fn workspace(layout: ViewerDiffLayout) -> TestResult<ClientDiffWorkspace> {
        let rows = match layout {
            ViewerDiffLayout::Unified => crate::entities::diffs::ClientDiffRows {
                unified: vec![
                    vec![ViewerUnifiedRow::Meta("one".into())],
                    vec![
                        ViewerUnifiedRow::Meta("two".into()),
                        ViewerUnifiedRow::Meta("three".into()),
                    ],
                ],
                split: Vec::new(),
            },
            ViewerDiffLayout::Split => ClientDiffRows {
                unified: Vec::new(),
                split: vec![
                    vec![ViewerSplitRow::Meta("one".into())],
                    vec![
                        ViewerSplitRow::Meta("two".into()),
                        ViewerSplitRow::Meta("three".into()),
                    ],
                ],
            },
        };
        Ok(ClientDiffWorkspace {
            identity: identity(layout)?,
            files: vec![ClientDiffFile {
                summary: ViewerFileSummary {
                    id: ViewerDiffFileId::for_index(0),
                    path: repository_relative_path("src/main.rs")?,
                    absolute_path: absolute_file_path("/repo/src/main.rs")?,
                    anchor_id: "file-0".into(),
                    added: DiffLineCount::new(1),
                    removed: DiffLineCount::default(),
                    status: ViewerFileStatus::Modified,
                    can_open_in_editor: true,
                    initially_expanded: false,
                    row_count: 3,
                },
                rows,
                line_number_digits: 1,
                state: ClientDiffFileState::Complete,
            }],
        })
    }

    #[test]
    fn validates_logical_row_indexes_before_rows_are_loaded() -> TestResult {
        for layout in [ViewerDiffLayout::Unified, ViewerDiffLayout::Split] {
            let mut workspace = workspace(layout)?;
            workspace.files[0].rows = ClientDiffRows::default();
            let identity = workspace.identity;
            let result = ViewerDiffSearchResult {
                identity,
                total_matches: 1,
                active_match: Some(ViewerDiffSearchMatch {
                    file: ViewerDiffFileId::for_index(0),
                    row_index: 2,
                }),
                wrapped: false,
            };

            let (_, row_target) = validate_diff_search_result(&workspace, identity, result)?;
            assert_eq!(row_target, Some((0, 2)));
        }
        Ok(())
    }

    #[test]
    fn rejects_out_of_bounds_or_inconsistent_search_results() -> TestResult {
        let workspace = workspace(ViewerDiffLayout::Unified)?;
        let identity = workspace.identity;
        let out_of_bounds = ViewerDiffSearchResult {
            identity,
            total_matches: 1,
            active_match: Some(ViewerDiffSearchMatch {
                file: ViewerDiffFileId::for_index(0),
                row_index: 3,
            }),
            wrapped: false,
        };
        let missing_match = ViewerDiffSearchResult {
            identity,
            total_matches: 1,
            active_match: None,
            wrapped: false,
        };

        assert_eq!(
            validate_diff_search_result(&workspace, identity, out_of_bounds).unwrap_err(),
            ViewerClientError::Internal
        );
        assert_eq!(
            validate_diff_search_result(&workspace, identity, missing_match).unwrap_err(),
            ViewerClientError::Internal
        );
        Ok(())
    }
}
