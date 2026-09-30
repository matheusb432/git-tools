use dioxus::prelude::*;
use gtl_wire::viewer::{
    VIEWER_SEARCH_QUERY_MAX_BYTES, ViewerDiffSearchDirection, ViewerViewIdentity,
};

use super::search_bar::{DiffSearchBar, DiffSearchScope};
use crate::{
    shared::{browser, i18n::use_language},
    views::diffs::{
        diff_workspace::file_filters::workspace::use_workspace_file_filters_context,
        text_filter::TextFilterState,
    },
};

const FIND_INPUT_ID: &str = "viewer-diff-find-input";

/// Edits the text filter from the document and steps through its matches.
///
/// Closing the bar keeps the filter; only the match highlight goes away.
#[component]
pub(super) fn DiffFindBar(
    open: Signal<bool>,
    identity: ViewerViewIdentity,
    target: Signal<Option<super::DiffSearchTarget>>,
) -> Element {
    let language = use_language();
    let text = use_workspace_file_filters_context().text;
    use_effect(move || {
        if open() {
            browser::focus_element(FIND_INPUT_ID.to_owned());
        }
    });
    use_effect(use_reactive((&identity,), move |(identity,)| {
        let state = text.state.read();
        let next = open().then(|| search_target(identity, &state)).flatten();
        if *target.peek() != next {
            target.set(next);
        }
    }));
    use_drop(browser::clear_diff_search_match);
    let close = move |()| {
        open.set(false);
        browser::focus_element("workspace-heading".to_owned());
    };

    if !open() {
        return rsx! {};
    }
    let state = text.state.read();
    rsx! {
        DiffSearchBar {
            input_id: FIND_INPUT_ID,
            scope: DiffSearchScope::AllFiles,
            query: text.query.read().clone(),
            status_message: state.message(language),
            navigation_enabled: state.navigation_enabled(),
            maxlength: VIEWER_SEARCH_QUERY_MAX_BYTES.to_string(),
            onquerychange: move |value| text.set_query.call(value),
            onprevious: move |()| text.navigate.call(ViewerDiffSearchDirection::Backward),
            onnext: move |()| text.navigate.call(ViewerDiffSearchDirection::Forward),
            onclose: close,
        }
    }
}

fn search_target(
    identity: ViewerViewIdentity,
    state: &TextFilterState,
) -> Option<super::DiffSearchTarget> {
    let found = state.active_match()?;
    Some(super::DiffSearchTarget {
        identity,
        file: found.file.clone(),
        row: usize::try_from(found.row_index).ok()?,
    })
}
