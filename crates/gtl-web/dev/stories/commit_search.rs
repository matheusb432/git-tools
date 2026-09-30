use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_wire::viewer::{ViewerCommitSummary, commit_search::ViewerCommitSearchResult};

use crate::views::commit_search::{CommitSearchInput, CommitSearchResults};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "w-52 bg-surface",
            CommitSearchInput {
                id: "commit-search-thumbnail",
                query: "fx auth",
                snapshot: true,
                onchange: |_| {},
            }
        }
    }
}

#[story]
fn snapshot() -> Element {
    let mut query = use_signal(|| "fx auth".to_owned());
    let mut branch = use_signal(|| false);
    rsx! {
        div { class: "w-52 bg-surface",
            CommitSearchInput {
                id: "commit-search-story",
                query: query(),
                snapshot: true,
                active_branch: branch(),
                onchange: move |value| query.set(value),
                onscope: move |value| branch.set(value),
            }
            CommitSearchResults {
                id: "commit-search-story",
                result: Some(results()),
                onselect: |_| {},
                onretry: |()| {},
            }
        }
    }
}

#[story]
fn pending() -> Element {
    rsx! {
        div { class: "w-52 bg-surface",
            CommitSearchInput {
                id: "commit-search-pending",
                query: "fx auth",
                snapshot: true,
                onchange: |_| {},
            }
            CommitSearchResults {
                id: "commit-search-pending",
                result: Some(results()),
                loading: true,
                onselect: |_| {},
                onretry: |()| {},
            }
        }
    }
}

#[story]
fn empty() -> Element {
    rsx! {
        div { class: "w-52 bg-surface",
            CommitSearchInput {
                id: "commit-search-empty",
                query: "unknown",
                snapshot: true,
                onchange: |_| {},
            }
            CommitSearchResults {
                id: "commit-search-empty",
                result: Some(
                    Ok(ViewerCommitSearchResult {
                        commits: Vec::new(),
                        total_matches: 0,
                        branch: None,
                    }),
                ),
                onselect: |_| {},
                onretry: |()| {},
            }
        }
    }
}

#[story(name = "Project search")]
fn project() -> Element {
    let mut query = use_signal(String::new);
    rsx! {
        div { class: "w-96 bg-surface",
            CommitSearchInput {
                id: "commit-search-project-story",
                query: query(),
                onchange: move |value| query.set(value),
            }
        }
    }
}

fn results() -> Result<ViewerCommitSearchResult, crate::shared::viewer_client::ViewerClientError> {
    use crate::shared::viewer_client::ViewerClientError;
    Ok(ViewerCommitSearchResult {
        commits: vec![ViewerCommitSummary {
            id: "1234567890abcdef1234567890abcdef12345678"
                .try_into()
                .map_err(|_| ViewerClientError::InvalidMessage)?,
            subject: "Fix authentication when the session expires".into(),
            body: String::new(),
            committed_at: "2026-09-29T12:00:00Z"
                .try_into()
                .map_err(|_| ViewerClientError::InvalidMessage)?,
            is_merge: false,
        }],
        total_matches: 1,
        branch: None,
    })
}

#[stories(id = "commit-search", name = "Commit search", thumbnail = thumbnail)]
const COMMIT_SEARCH_STORIES: () = &[snapshot, pending, empty, project];
