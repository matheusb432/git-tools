use dioxus::prelude::*;
use gtl_wire::viewer::{
    FindViewerDiff, VIEWER_SEARCH_QUERY_MAX_BYTES, ViewerDiffLayout, ViewerDiffSearchDirection,
    ViewerDiffSearchMatch, ViewerDiffSearchResult, ViewerViewIdentity,
};

use crate::{
    entities::diffs::{ClientDiffWorkspace, viewer_server},
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, TextInput, TextInputLabelVisibility},
        viewer_client::ViewerClientError,
    },
};

const FIND_INPUT_ID: &str = "viewer-diff-find-input";
const FIND_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffFindRequest {
    generation: u64,
    identity: ViewerViewIdentity,
    query: String,
    direction: ViewerDiffSearchDirection,
    anchor: Option<ViewerDiffSearchMatch>,
    debounce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffFindState {
    Idle,
    WaitingForRows,
    QueryTooLong,
    Loading,
    Ready(ViewerDiffSearchResult),
    Error(ViewerClientError),
}

impl DiffFindState {
    fn active_match(&self) -> Option<ViewerDiffSearchMatch> {
        match self {
            Self::Ready(result) => result.active_match.clone(),
            Self::Idle
            | Self::WaitingForRows
            | Self::QueryTooLong
            | Self::Loading
            | Self::Error(_) => None,
        }
    }

    fn navigation_enabled(&self) -> bool {
        matches!(self, Self::Ready(result) if result.total_matches > 0)
    }

    fn message(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::WaitingForRows => "Search is available when the diff finishes loading.".into(),
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

#[component]
pub(super) fn DiffFindBar(
    mut open: Signal<bool>,
    identity: ViewerViewIdentity,
    rows_loading: bool,
    workspace: ReadStore<ClientDiffWorkspace>,
) -> Element {
    let mut query = use_signal(String::new);
    let request = use_signal(|| None::<DiffFindRequest>);
    let state = use_signal(|| DiffFindState::Idle);
    let _search = use_resource(move || {
        let next = request();
        async move {
            let Some(next) = next else {
                return;
            };
            execute_find_request(next, request, state, workspace).await;
        }
    });

    use_effect(move || {
        if open() {
            browser::focus_element(FIND_INPUT_ID.to_owned());
        } else {
            browser::clear_diff_search_match();
        }
    });
    use_effect(use_reactive(
        (&identity, &rows_loading),
        move |(identity, rows_loading)| {
            browser::clear_diff_search_match();
            queue_initial_search(request, state, identity, query.peek().clone(), rows_loading);
        },
    ));
    use_drop(browser::clear_diff_search_match);

    if !open() {
        return rsx! {};
    }
    let current_state = state.read().clone();
    let status_message = current_state.message();
    let navigation_state = if current_state.navigation_enabled() {
        ButtonState::Enabled
    } else {
        ButtonState::Disabled
    };

    rsx! {
        div {
            class: "absolute top-3 right-5 z-20 grid w-[min(29rem,calc(100%-1.5rem))] grid-cols-[minmax(0,1fr)_auto_auto_auto] items-start gap-1.5 rounded-panel border border-line-2 bg-surface p-2 shadow-lg tablet:right-3 mobile:top-1 mobile:right-1",
            role: "search",
            aria_label: "Find in diff",
            onkeydown: move |event: KeyboardEvent| {
                match event.key() {
                    Key::Escape => {
                        event.prevent_default();
                        open.set(false);
                        browser::clear_diff_search_match();
                        browser::focus_element("workspace-heading".to_owned());
                    }
                    Key::Enter => {
                        event.prevent_default();
                        let direction = if event.modifiers().shift() {
                            ViewerDiffSearchDirection::Backward
                        } else {
                            ViewerDiffSearchDirection::Forward
                        };
                        queue_navigation(
                            request,
                            state,
                            identity,
                            query.peek().clone(),
                            direction,
                        );
                    }
                    _ => {}
                }
            },
            div { class: "min-w-0",
                TextInput {
                    id: FIND_INPUT_ID,
                    label: "Find in diff",
                    label_visibility: TextInputLabelVisibility::Hidden,
                    class: "h-8 py-1.5",
                    value: query(),
                    maxlength: VIEWER_SEARCH_QUERY_MAX_BYTES.to_string(),
                    placeholder: "Find in diff\u{2026}",
                    oninput: move |event: FormEvent| {
                        let value = event.value();
                        query.set(value.clone());
                        browser::clear_diff_search_match();
                        queue_initial_search(request, state, identity, value, rows_loading);
                    },
                }
                p {
                    class: "mt-1 min-h-4 px-0.5 text-xs text-ink-3",
                    role: "status",
                    aria_live: "polite",
                    "{status_message}"
                }
            }
            Button {
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                state: navigation_state,
                aria_label: "Previous match",
                title: "Previous match (Shift+Enter)",
                onclick: move |_| queue_navigation(
                    request,
                    state,
                    identity,
                    query.peek().clone(),
                    ViewerDiffSearchDirection::Backward,
                ),
                span { aria_hidden: "true", "\u{2191}" }
            }
            Button {
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                state: navigation_state,
                aria_label: "Next match",
                title: "Next match (Enter)",
                onclick: move |_| queue_navigation(
                    request,
                    state,
                    identity,
                    query.peek().clone(),
                    ViewerDiffSearchDirection::Forward,
                ),
                span { aria_hidden: "true", "\u{2193}" }
            }
            Button {
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                aria_label: "Close find",
                title: "Close find (Escape)",
                onclick: move |_| {
                    open.set(false);
                    browser::clear_diff_search_match();
                    browser::focus_element("workspace-heading".to_owned());
                },
                span { aria_hidden: "true", "\u{00d7}" }
            }
        }
    }
}

fn queue_initial_search(
    mut request: Signal<Option<DiffFindRequest>>,
    mut state: Signal<DiffFindState>,
    identity: ViewerViewIdentity,
    query: String,
    rows_loading: bool,
) {
    if query.is_empty() {
        request.set(None);
        state.set(DiffFindState::Idle);
        return;
    }
    if query.len() > VIEWER_SEARCH_QUERY_MAX_BYTES {
        request.set(None);
        state.set(DiffFindState::QueryTooLong);
        return;
    }
    if rows_loading {
        request.set(None);
        state.set(DiffFindState::WaitingForRows);
        return;
    }
    queue_search(
        request,
        state,
        identity,
        query,
        ViewerDiffSearchDirection::Forward,
        None,
        true,
    );
}

fn queue_navigation(
    request: Signal<Option<DiffFindRequest>>,
    state: Signal<DiffFindState>,
    identity: ViewerViewIdentity,
    query: String,
    direction: ViewerDiffSearchDirection,
) {
    if query.is_empty() || query.len() > VIEWER_SEARCH_QUERY_MAX_BYTES {
        return;
    }
    let anchor = state.peek().active_match();
    queue_search(request, state, identity, query, direction, anchor, false);
}

fn queue_search(
    mut request: Signal<Option<DiffFindRequest>>,
    mut state: Signal<DiffFindState>,
    identity: ViewerViewIdentity,
    query: String,
    direction: ViewerDiffSearchDirection,
    anchor: Option<ViewerDiffSearchMatch>,
    debounce: bool,
) {
    let generation = request
        .peek()
        .as_ref()
        .map_or(1, |request| request.generation.wrapping_add(1));
    state.set(DiffFindState::Loading);
    request.set(Some(DiffFindRequest {
        generation,
        identity,
        query,
        direction,
        anchor,
        debounce,
    }));
}

async fn execute_find_request(
    next: DiffFindRequest,
    request: Signal<Option<DiffFindRequest>>,
    mut state: Signal<DiffFindState>,
    workspace: ReadStore<ClientDiffWorkspace>,
) {
    if next.debounce {
        dioxus_sdk_time::sleep(FIND_DEBOUNCE).await;
    }
    if request.peek().as_ref() != Some(&next) {
        return;
    }
    let result = viewer_server::find_diff(FindViewerDiff {
        identity: next.identity,
        query: next.query.clone(),
        direction: next.direction,
        anchor: next.anchor.clone(),
    })
    .await
    .and_then(|result| validate_diff_search_result(&workspace.peek(), next.identity, result));
    if request.peek().as_ref() != Some(&next) {
        return;
    }
    match result {
        Ok((result, row_target)) => {
            if let Some((file_index, row_index)) = row_target
                && !browser::show_diff_search_match(file_index, row_index)
            {
                state.set(DiffFindState::Error(ViewerClientError::Internal));
                return;
            }
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
        .map(|found| diff_search_match_target(workspace, identity, found))
        .transpose()?;
    Ok((result, row_target))
}

fn diff_search_match_target(
    workspace: &ClientDiffWorkspace,
    identity: ViewerViewIdentity,
    found: &ViewerDiffSearchMatch,
) -> Result<(usize, usize), ViewerClientError> {
    let file_index = workspace
        .files
        .iter()
        .position(|file| file.summary.id == found.file)
        .ok_or(ViewerClientError::Internal)?;
    let row_count = match identity.render_options.layout {
        ViewerDiffLayout::Unified => workspace.files[file_index]
            .rows
            .unified
            .iter()
            .map(Vec::len)
            .sum(),
        ViewerDiffLayout::Split => workspace.files[file_index]
            .rows
            .split
            .iter()
            .map(Vec::len)
            .sum(),
    };
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
        ViewerDiffDensity, ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary,
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
                },
                rows,
                line_number_digits: 1,
                state: ClientDiffFileState::Complete,
            }],
        })
    }

    #[test]
    fn validates_logical_row_indexes_across_variable_batches() -> TestResult {
        for layout in [ViewerDiffLayout::Unified, ViewerDiffLayout::Split] {
            let workspace = workspace(layout)?;
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
