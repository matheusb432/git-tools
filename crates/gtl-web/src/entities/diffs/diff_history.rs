use gtl_wire::viewer::{ViewerHistoryCursor, ViewerHistoryPage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HistoryNavigation {
    pub(crate) previous: Option<ViewerHistoryCursor>,
    pub(crate) next: Option<ViewerHistoryCursor>,
}

pub(crate) fn history_navigation(page: &ViewerHistoryPage) -> HistoryNavigation {
    let first_entry = page.entries.first();
    let last_entry = page.entries.last();
    let history_page = page.position.page();
    let previous_page = history_page.and_then(gtl_models::viewer::HistoryPage::previous);
    let next_page = history_page.and_then(gtl_models::viewer::HistoryPage::next);

    HistoryNavigation {
        previous: page
            .has_newer
            .then_some(())
            .and_then(|()| first_entry.zip(previous_page))
            .map(|(entry, page)| ViewerHistoryCursor::NewerThan {
                render_id: entry.id,
                page,
            }),
        next: page
            .has_older
            .then_some(())
            .and_then(|()| last_entry.zip(next_page))
            .map(|(entry, page)| ViewerHistoryCursor::OlderThan {
                render_id: entry.id,
                page,
            }),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{HistoryPagePosition, HistoryRenderCount};
    use gtl_wire::viewer::{
        ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage, ViewerRecipeKind,
    };

    use super::history_navigation;
    use crate::test_support::{
        TestResult, history_page, history_page_number, machine_timestamp, project_name,
        render_history_id,
    };

    fn entry(id: i64) -> TestResult<ViewerHistoryEntry> {
        Ok(ViewerHistoryEntry {
            id: render_history_id(id)?,
            title: format!("Render {id}"),
            repository_name: project_name("git-tools")?,
            kind: ViewerRecipeKind::Diff,
            range_label: "main..HEAD".into(),
            rendered_at: machine_timestamp("2026-08-09T00:00:00Z")?,
        })
    }

    #[test]
    fn navigation_uses_page_edge_ids_for_adjacent_pages() -> TestResult {
        let page = ViewerHistoryPage {
            entries: vec![entry(90)?, entry(81)?],
            total_count: HistoryRenderCount::new(42),
            position: HistoryPagePosition::Page(history_page(3, 5)?),
            has_newer: true,
            has_older: true,
        };

        let navigation = history_navigation(&page);

        assert_eq!(
            navigation.previous,
            Some(ViewerHistoryCursor::NewerThan {
                render_id: render_history_id(90)?,
                page: history_page_number(2)?,
            })
        );
        assert_eq!(
            navigation.next,
            Some(ViewerHistoryCursor::OlderThan {
                render_id: render_history_id(81)?,
                page: history_page_number(4)?,
            })
        );
        Ok(())
    }
}
