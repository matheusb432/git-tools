use gtl_contracts::viewer::{ViewerHistoryCursor, ViewerHistoryPage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HistoryNavigation {
    pub(crate) first: Option<ViewerHistoryCursor>,
    pub(crate) previous: Option<ViewerHistoryCursor>,
    pub(crate) next: Option<ViewerHistoryCursor>,
    pub(crate) last: Option<ViewerHistoryCursor>,
}

pub(crate) fn history_navigation(page: &ViewerHistoryPage) -> HistoryNavigation {
    let first_entry = page.entries.first();
    let last_entry = page.entries.last();
    let previous_page = page.page_number.checked_sub(1);
    let next_page = page.page_number.checked_add(1);

    HistoryNavigation {
        first: page.has_newer.then_some(ViewerHistoryCursor::Newest),
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
        last: page.has_older.then_some(ViewerHistoryCursor::Oldest),
    }
}

#[cfg(test)]
mod tests {
    use gtl_contracts::{
        recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
        viewer::{ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage, ViewerRecipeKind},
    };

    use super::history_navigation;

    fn entry(id: i64) -> ViewerHistoryEntry {
        ViewerHistoryEntry {
            id,
            title: format!("Render {id}"),
            repository_name: "git-tools".into(),
            kind: ViewerRecipeKind::Diff,
            range_label: "main..HEAD".into(),
            rendered_at: "2026-08-09T00:00:00Z".into(),
            recipe: Recipe {
                source: RecipeSource::LocalRepo("/repo".into()),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: None,
            },
        }
    }

    #[test]
    fn navigation_uses_page_edge_ids_for_all_four_directions() {
        let page = ViewerHistoryPage {
            entries: vec![entry(90), entry(81)],
            total_count: 42,
            page_number: 3,
            page_count: 5,
            has_newer: true,
            has_older: true,
        };

        let navigation = history_navigation(&page);

        assert_eq!(navigation.first, Some(ViewerHistoryCursor::Newest));
        assert_eq!(
            navigation.previous,
            Some(ViewerHistoryCursor::NewerThan {
                render_id: 90,
                page: 2,
            })
        );
        assert_eq!(
            navigation.next,
            Some(ViewerHistoryCursor::OlderThan {
                render_id: 81,
                page: 4,
            })
        );
        assert_eq!(navigation.last, Some(ViewerHistoryCursor::Oldest));
    }
}
