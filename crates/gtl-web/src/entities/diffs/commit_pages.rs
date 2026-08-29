use dioxus::prelude::*;
use gtl_models::viewer::{ViewerRangeGeneration, ViewerTabId};
use gtl_wire::viewer::{
    ListViewerCommits, ViewerCommitCursor, ViewerCommitPage, ViewerCommitSummary,
    ViewerViewIdentity,
};

use super::viewer_server;
use crate::shared::viewer_client::ViewerClientError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerCommitListKey {
    tab_id: ViewerTabId,
    range_generation: ViewerRangeGeneration,
}

impl From<ViewerViewIdentity> for ViewerCommitListKey {
    fn from(identity: ViewerViewIdentity) -> Self {
        Self {
            tab_id: identity.tab_id,
            range_generation: identity.range_generation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerCommitPageRequest {
    Idle,
    Loading,
    Error(ViewerClientError),
}

impl ViewerCommitPageRequest {
    const fn is_loading(self) -> bool {
        matches!(self, Self::Loading)
    }

    const fn error(self) -> Option<ViewerClientError> {
        match self {
            Self::Error(error) => Some(error),
            Self::Idle | Self::Loading => None,
        }
    }
}

#[derive(Debug, Store, Clone, PartialEq, Eq)]
pub(crate) struct ViewerCommitPages {
    key: ViewerCommitListKey,
    expected_count: usize,
    commits: Vec<ViewerCommitSummary>,
    next_cursor: Option<ViewerCommitCursor>,
    request: ViewerCommitPageRequest,
}

impl ViewerCommitPages {
    fn new(identity: ViewerViewIdentity, expected_count: usize) -> Self {
        Self {
            key: identity.into(),
            expected_count,
            commits: Vec::new(),
            next_cursor: (expected_count > 0).then(ViewerCommitCursor::default),
            request: ViewerCommitPageRequest::Idle,
        }
    }

    pub(crate) const fn is_loading(&self) -> bool {
        self.request.is_loading()
    }

    fn request(&self) -> Option<(ViewerCommitListKey, ViewerCommitCursor)> {
        if self.is_loading() {
            return None;
        }
        self.next_cursor.map(|cursor| (self.key, cursor))
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ViewerCommitPagesController {
    pages: Store<ViewerCommitPages>,
    commits: ReadStore<Vec<ViewerCommitSummary>>,
    request: Resource<()>,
    request_generation: Signal<u64>,
}

impl ViewerCommitPagesController {
    pub(crate) fn commits(self) -> ReadStore<Vec<ViewerCommitSummary>> {
        self.commits
    }

    pub(crate) fn is_loading(self) -> bool {
        self.pages.request().read().is_loading()
    }

    pub(crate) fn error(self) -> Option<ViewerClientError> {
        self.pages.request().read().error()
    }

    pub(crate) fn has_more(self) -> bool {
        self.pages.next_cursor().read().is_some()
    }

    pub(crate) fn load_next(mut self) {
        if self.request.pending() || self.pages.peek().request().is_none() {
            return;
        }
        let mut generation = self.request_generation.write();
        *generation = generation.wrapping_add(1);
    }
}

pub(crate) fn use_viewer_commit_pages(
    view: ReadSignal<gtl_wire::viewer::ViewerActiveView>,
) -> ViewerCommitPagesController {
    let list = use_memo(move || {
        let view = view.read();
        (ViewerCommitListKey::from(view.identity), view.commit_count)
    });
    let mut pages = use_store(move || {
        let view = view.peek();
        ViewerCommitPages::new(view.identity, view.commit_count)
    });
    let request_generation = use_signal(|| 0_u64);
    let request = use_resource(move || {
        let (expected_key, expected_count) = list();
        let _request_generation = request_generation();
        let identity = view.peek().identity;
        if pages.peek().key != expected_key || pages.peek().expected_count != expected_count {
            pages.set(ViewerCommitPages::new(identity, expected_count));
        }
        let page_request = pages.peek().request();
        if page_request.is_some() {
            pages.request().set(ViewerCommitPageRequest::Loading);
        }

        request_next_page(pages, identity, page_request)
    });
    let commits: ReadStore<Vec<ViewerCommitSummary>> = use_hook(move || pages.commits().into());

    ViewerCommitPagesController {
        pages,
        commits,
        request,
        request_generation,
    }
}

async fn request_next_page(
    pages: Store<ViewerCommitPages>,
    identity: ViewerViewIdentity,
    page_request: Option<(ViewerCommitListKey, ViewerCommitCursor)>,
) {
    let Some((key, cursor)) = page_request else {
        return;
    };
    let result = viewer_server::list_commits(ListViewerCommits {
        identity,
        cursor: Some(cursor),
    })
    .await;
    finish_page_request(pages, key, cursor, result);
}

fn finish_page_request(
    pages: Store<ViewerCommitPages>,
    key: ViewerCommitListKey,
    cursor: ViewerCommitCursor,
    result: Result<ViewerCommitPage, ViewerClientError>,
) {
    if pages.peek().key != key || !pages.peek().is_loading() {
        return;
    }
    match result.and_then(|page| validate_page(&pages.peek(), cursor, page)) {
        Ok(page) => {
            let mut commits = pages.commits();
            commits.write().extend(page.commits);
            pages.next_cursor().set(page.next_cursor);
            pages.request().set(ViewerCommitPageRequest::Idle);
        }
        Err(error) => pages.request().set(ViewerCommitPageRequest::Error(error)),
    }
}

fn validate_page(
    current: &ViewerCommitPages,
    cursor: ViewerCommitCursor,
    page: ViewerCommitPage,
) -> Result<ViewerCommitPage, ViewerClientError> {
    if ViewerCommitListKey::from(page.identity) != current.key {
        return Err(ViewerClientError::Internal);
    }
    let cursor = usize::try_from(cursor.into_inner()).map_err(|_| ViewerClientError::Internal)?;
    if cursor != current.commits.len() {
        return Err(ViewerClientError::Internal);
    }
    let loaded_count = cursor
        .checked_add(page.commits.len())
        .ok_or(ViewerClientError::Internal)?;
    if loaded_count > current.expected_count {
        return Err(ViewerClientError::Internal);
    }
    match page.next_cursor {
        Some(next_cursor)
            if usize::try_from(next_cursor.into_inner()).ok() == Some(loaded_count)
                && loaded_count < current.expected_count => {}
        None if loaded_count == current.expected_count => {}
        Some(_) | None => return Err(ViewerClientError::Internal),
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::CommitId,
        timestamps::MachineTimestamp,
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
    };
    use gtl_wire::viewer::{
        ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions, ViewerViewIdentity,
    };

    use super::*;
    use crate::test_support::{TestResult, viewer_tab_id};

    fn identity(selection_generation: u64) -> TestResult<ViewerViewIdentity> {
        Ok(ViewerViewIdentity {
            tab_id: viewer_tab_id(7)?,
            range_generation: ViewerRangeGeneration::new(11),
            selection_generation: ViewerSelectionGeneration::new(selection_generation),
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        })
    }

    fn commit(digit: char) -> TestResult<ViewerCommitSummary> {
        Ok(ViewerCommitSummary {
            id: CommitId::try_from(digit.to_string().repeat(40))?,
            subject: format!("Commit {digit}"),
            body: String::new(),
            committed_at: MachineTimestamp::try_from("2026-08-24T12:00:00Z")?,
            is_merge: false,
        })
    }

    #[test]
    fn commit_list_key_does_not_change_when_commit_selection_changes() -> TestResult {
        assert_eq!(
            ViewerCommitListKey::from(identity(0)?),
            ViewerCommitListKey::from(identity(1)?)
        );
        Ok(())
    }

    #[test]
    fn page_cursor_must_equal_the_loaded_commit_count() -> TestResult {
        let identity = identity(0)?;
        let current = ViewerCommitPages::new(identity, 2);
        let page = ViewerCommitPage {
            identity,
            commits: vec![commit('a')?],
            next_cursor: Some(ViewerCommitCursor::new(1)),
        };

        assert_eq!(
            validate_page(&current, ViewerCommitCursor::default(), page.clone()),
            Ok(page.clone())
        );
        assert_eq!(
            validate_page(&current, ViewerCommitCursor::new(1), page),
            Err(ViewerClientError::Internal)
        );
        Ok(())
    }

    #[test]
    fn final_page_must_reach_the_shell_commit_count() -> TestResult {
        let identity = identity(0)?;
        let current = ViewerCommitPages::new(identity, 2);
        let incomplete_final_page = ViewerCommitPage {
            identity,
            commits: vec![commit('a')?],
            next_cursor: None,
        };

        assert_eq!(
            validate_page(
                &current,
                ViewerCommitCursor::default(),
                incomplete_final_page
            ),
            Err(ViewerClientError::Internal)
        );
        Ok(())
    }
}
