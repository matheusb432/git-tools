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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewerCommitPages {
    key: ViewerCommitListKey,
    expected_count: usize,
    pub(crate) commits: Vec<ViewerCommitSummary>,
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
        matches!(self.request, ViewerCommitPageRequest::Loading)
    }

    pub(crate) const fn error(&self) -> Option<ViewerClientError> {
        match self.request {
            ViewerCommitPageRequest::Error(error) => Some(error),
            ViewerCommitPageRequest::Idle | ViewerCommitPageRequest::Loading => None,
        }
    }

    pub(crate) const fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }

    fn begin(&mut self) -> Option<(ViewerCommitListKey, ViewerCommitCursor)> {
        if self.is_loading() {
            return None;
        }
        let cursor = self.next_cursor?;
        self.request = ViewerCommitPageRequest::Loading;
        Some((self.key, cursor))
    }

    fn finish(
        &mut self,
        key: ViewerCommitListKey,
        cursor: ViewerCommitCursor,
        result: Result<ViewerCommitPage, ViewerClientError>,
    ) {
        if self.key != key || !self.is_loading() {
            return;
        }
        match result.and_then(|page| validate_page(self, cursor, page)) {
            Ok(page) => {
                self.commits.extend(page.commits);
                self.next_cursor = page.next_cursor;
                self.request = ViewerCommitPageRequest::Idle;
            }
            Err(error) => self.request = ViewerCommitPageRequest::Error(error),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ViewerCommitPagesController {
    pages: Signal<ViewerCommitPages>,
}

impl ViewerCommitPagesController {
    pub(crate) fn read(self) -> ViewerCommitPages {
        (self.pages)()
    }

    pub(crate) fn load_next(mut self, identity: ViewerViewIdentity) {
        let Some((key, cursor)) = self.pages.write().begin() else {
            return;
        };
        spawn(async move {
            let result = viewer_server::list_commits(ListViewerCommits {
                identity,
                cursor: Some(cursor),
            })
            .await;
            self.pages.write().finish(key, cursor, result);
        });
    }
}

pub(crate) fn use_viewer_commit_pages(
    identity: ViewerViewIdentity,
    expected_count: usize,
) -> ViewerCommitPagesController {
    let key = ViewerCommitListKey::from(identity);
    let pages = use_signal(move || ViewerCommitPages::new(identity, expected_count));
    let controller = ViewerCommitPagesController { pages };

    use_effect(use_reactive(
        (&key, &expected_count),
        move |(_key, expected_count)| {
            let mut controller = controller;
            controller
                .pages
                .set(ViewerCommitPages::new(identity, expected_count));
            controller.load_next(identity);
        },
    ));

    controller
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
