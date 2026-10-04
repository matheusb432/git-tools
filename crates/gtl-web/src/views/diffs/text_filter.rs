use std::{collections::HashSet, time::Duration};

use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::{
    FindViewerDiff, VIEWER_SEARCH_QUERY_MAX_BYTES, ViewerActiveView, ViewerDiffFileId,
    ViewerDiffSearchDirection, ViewerDiffSearchMatch, ViewerDiffSearchResult, ViewerViewIdentity,
};

use crate::{
    entities::diffs::viewer_server,
    shared::{failure_notice::client_error_message, i18n::t, viewer_client::ViewerClientError},
};

const TEXT_FILTER_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSearchKey {
    identity: ViewerViewIdentity,
    query: String,
    files: Vec<ViewerDiffFileId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSearchOutcome {
    key: TextSearchKey,
    result: Result<ViewerDiffSearchResult, ViewerClientError>,
}

impl TextSearchOutcome {
    fn state(&self) -> TextFilterState {
        match &self.result {
            Ok(result) => TextFilterState::Ready(result.clone()),
            Err(error) => TextFilterState::Error(error.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextFilterState {
    Inactive,
    QueryTooLong,
    Searching,
    Ready(ViewerDiffSearchResult),
    Error(ViewerClientError),
}

impl TextFilterState {
    pub(crate) fn active_match(&self) -> Option<&ViewerDiffSearchMatch> {
        match self {
            Self::Ready(result) => result.active_match.as_ref(),
            Self::Inactive | Self::QueryTooLong | Self::Searching | Self::Error(_) => None,
        }
    }

    pub(crate) fn navigation_enabled(&self) -> bool {
        matches!(self, Self::Ready(result) if result.total_matches > 0)
    }

    pub(crate) fn message(&self, language: ViewerLanguage) -> String {
        match self {
            Self::Inactive => String::new(),
            Self::QueryTooLong => t!(
                language,
                "diff-find-query-too-long",
                bytes = VIEWER_SEARCH_QUERY_MAX_BYTES
            ),
            Self::Searching => t!(language, "diff-find-searching"),
            Self::Ready(result) if result.total_matches == 0 => {
                t!(language, "diff-find-no-matches")
            }
            Self::Ready(result) if result.wrapped => t!(
                language,
                "diff-find-matches-wrapped",
                count = result.total_matches
            ),
            Self::Ready(result) => t!(language, "diff-find-matches", count = result.total_matches),
            Self::Error(error) => client_error_message(error, language),
        }
    }
}

/// Shows only files whose rows contain the query and steps through their matches.
#[derive(Clone, Copy)]
pub(crate) struct TextFilter {
    pub(crate) query: Memo<String>,
    pub(crate) state: Memo<TextFilterState>,
    /// The latest matches for the displayed identity; `None` leaves the files unfiltered.
    pub(crate) matched_files: Memo<Option<HashSet<ViewerDiffFileId>>>,
    pub(crate) set_query: Callback<String>,
    pub(crate) navigate: Callback<ViewerDiffSearchDirection>,
}

pub(crate) fn use_text_filter(
    source: ReadSignal<ViewerActiveView>,
    query: Memo<String>,
    files: Memo<Vec<ViewerDiffFileId>>,
    set_query: Callback<String>,
) -> TextFilter {
    let key = use_memo(move || {
        let query = query();
        (!query.is_empty() && query.len() <= VIEWER_SEARCH_QUERY_MAX_BYTES).then(|| TextSearchKey {
            identity: source.read().identity,
            query,
            files: files(),
        })
    });
    let settled_query = use_signal(|| None::<String>);
    let search = use_resource(move || search_text(key(), settled_query, source));
    let mut navigation = use_signal(|| None::<TextSearchOutcome>);
    let state = use_memo(move || {
        let search = search.read();
        text_filter_state(
            key.read().as_ref(),
            &query.read(),
            navigation.read().as_ref(),
            search.as_ref().and_then(Option::as_ref),
        )
    });
    let matched_files = use_memo(move || {
        key.read().as_ref()?;
        let search = search.read();
        let outcome = search.as_ref()?.as_ref()?;
        if outcome.key.identity != source.read().identity {
            return None;
        }
        let result = outcome.result.as_ref().ok()?;
        Some(result.matched_files.iter().cloned().collect())
    });
    let mut step = use_action(move |direction: ViewerDiffSearchDirection| async move {
        let key = key.peek().clone();
        let anchor = state.peek().active_match().cloned();
        if let Some(key) = key {
            let result = find(&key, anchor, direction, source).await;
            navigation.set(Some(TextSearchOutcome { key, result }));
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let navigate = use_callback(move |direction| {
        if state.peek().navigation_enabled() {
            step.call(direction);
        }
    });
    TextFilter {
        query,
        state,
        matched_files,
        set_query,
        navigate,
    }
}

async fn search_text(
    key: Option<TextSearchKey>,
    mut settled_query: Signal<Option<String>>,
    source: ReadSignal<ViewerActiveView>,
) -> Option<TextSearchOutcome> {
    let key = key?;
    if settled_query.peek().as_ref() != Some(&key.query) {
        dioxus_sdk_time::sleep(TEXT_FILTER_DEBOUNCE).await;
    }
    let result = find(&key, None, ViewerDiffSearchDirection::Forward, source).await;
    settled_query.set(Some(key.query.clone()));
    Some(TextSearchOutcome { key, result })
}

/// Prefers the latest navigation over the initial search when both answer the current key.
fn text_filter_state(
    key: Option<&TextSearchKey>,
    query: &str,
    navigation: Option<&TextSearchOutcome>,
    search: Option<&TextSearchOutcome>,
) -> TextFilterState {
    let Some(key) = key else {
        return if query.is_empty() {
            TextFilterState::Inactive
        } else {
            TextFilterState::QueryTooLong
        };
    };
    navigation
        .filter(|outcome| &outcome.key == key)
        .or_else(|| search.filter(|outcome| &outcome.key == key))
        .map_or(TextFilterState::Searching, TextSearchOutcome::state)
}

async fn find(
    key: &TextSearchKey,
    anchor: Option<ViewerDiffSearchMatch>,
    direction: ViewerDiffSearchDirection,
    source: ReadSignal<ViewerActiveView>,
) -> Result<ViewerDiffSearchResult, ViewerClientError> {
    let result = viewer_server::find_diff(FindViewerDiff {
        identity: key.identity,
        files: key.files.clone(),
        query: key.query.clone(),
        direction,
        anchor,
    })
    .await?;
    validate_search_result(&source.peek(), key, result)
}

fn validate_search_result(
    source: &ViewerActiveView,
    key: &TextSearchKey,
    result: ViewerDiffSearchResult,
) -> Result<ViewerDiffSearchResult, ViewerClientError> {
    let searched = key.files.iter().collect::<HashSet<_>>();
    let matched = result.matched_files.iter().collect::<HashSet<_>>();
    let found = result.total_matches > 0;
    let consistent = result.identity == key.identity
        && matched.len() == result.matched_files.len()
        && matched.is_subset(&searched)
        && found == result.active_match.is_some()
        && found != matched.is_empty();
    if !consistent {
        return Err(ViewerClientError::InvalidMessage);
    }
    let Some(active) = &result.active_match else {
        return Ok(result);
    };
    let row_count = source
        .files
        .iter()
        .find(|file| file.id == active.file)
        .map(|file| file.row_count);
    let row_is_valid = usize::try_from(active.row_index)
        .ok()
        .zip(row_count)
        .is_some_and(|(row, row_count)| row < row_count);
    if !matched.contains(&active.file) || (source.identity == key.identity && !row_is_valid) {
        return Err(ViewerClientError::InvalidMessage);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerFileStatus;

    use super::*;
    use crate::test_support::{TestResult, viewer_active_view, viewer_file_summary, viewer_tab_id};

    fn source() -> TestResult<ViewerActiveView> {
        let mut view = viewer_active_view(viewer_tab_id(3)?);
        view.files = vec![
            viewer_file_summary(0, "src/a.rs", ViewerFileStatus::Modified, 1, 1)?,
            viewer_file_summary(1, "src/b.rs", ViewerFileStatus::Added, 2, 0)?,
        ];
        view.files[1].row_count = 5;
        Ok(view)
    }

    fn key(source: &ViewerActiveView) -> TextSearchKey {
        TextSearchKey {
            identity: source.identity,
            query: "needle".to_owned(),
            files: vec![ViewerDiffFileId::for_index(1)],
        }
    }

    fn result(
        source: &ViewerActiveView,
        file: ViewerDiffFileId,
        row_index: u32,
    ) -> ViewerDiffSearchResult {
        ViewerDiffSearchResult {
            identity: source.identity,
            total_matches: 1,
            active_match: Some(ViewerDiffSearchMatch {
                file: file.clone(),
                row_index,
            }),
            wrapped: false,
            matched_files: vec![file],
        }
    }

    #[test]
    fn navigation_answers_only_its_own_key() -> TestResult {
        let source = source()?;
        let key = key(&source);
        let initial = TextSearchOutcome {
            key: key.clone(),
            result: Ok(result(&source, ViewerDiffFileId::for_index(1), 1)),
        };
        let stepped = TextSearchOutcome {
            key: key.clone(),
            result: Ok(result(&source, ViewerDiffFileId::for_index(1), 3)),
        };
        let stale = TextSearchOutcome {
            key: TextSearchKey {
                query: "needl".to_owned(),
                ..key.clone()
            },
            ..stepped.clone()
        };

        assert_eq!(
            text_filter_state(Some(&key), &key.query, Some(&stepped), Some(&initial)),
            stepped.state()
        );
        assert_eq!(
            text_filter_state(Some(&key), &key.query, Some(&stale), Some(&initial)),
            initial.state()
        );
        assert_eq!(
            text_filter_state(Some(&key), &key.query, None, Some(&stale)),
            TextFilterState::Searching
        );
        assert_eq!(
            text_filter_state(None, "", None, None),
            TextFilterState::Inactive
        );
        Ok(())
    }

    #[test]
    fn accepts_matches_inside_the_searched_files_and_rows() -> TestResult {
        let source = source()?;
        let found = result(&source, ViewerDiffFileId::for_index(1), 4);

        assert_eq!(
            validate_search_result(&source, &key(&source), found.clone()),
            Ok(found)
        );
        Ok(())
    }

    #[test]
    fn rejects_matches_outside_the_searched_files_or_rows() -> TestResult {
        let source = source()?;
        let key = key(&source);

        for invalid in [
            result(&source, ViewerDiffFileId::for_index(0), 0),
            result(&source, ViewerDiffFileId::for_index(1), 5),
            ViewerDiffSearchResult {
                active_match: None,
                ..result(&source, ViewerDiffFileId::for_index(1), 0)
            },
        ] {
            assert_eq!(
                validate_search_result(&source, &key, invalid),
                Err(ViewerClientError::InvalidMessage)
            );
        }
        Ok(())
    }
}
