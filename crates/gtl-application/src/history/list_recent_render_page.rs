//! The keyset-paginated query for the desktop viewer's recent render history.

use gtl_models::viewer::{
    HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
    RenderHistoryId,
};
use rusqlite::{Connection, params_from_iter};

use crate::history::{
    RecentRenderRecord, RecentRenderRowError,
    persistence::{RECENT_RENDER_SELECT, RecentRenderRow},
};

pub const RECENT_RENDER_PAGE_SIZE: usize = 30;
const RECENT_RENDER_PAGE_QUERY_SIZE: usize = RECENT_RENDER_PAGE_SIZE + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecentRenderPageCursor {
    #[default]
    Newest,
    OlderThan {
        render: RenderHistoryId,
        page: HistoryPageNumber,
    },
    NewerThan {
        render: RenderHistoryId,
        page: HistoryPageNumber,
    },
    Oldest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListRecentRenderPage {
    pub cursor: RecentRenderPageCursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentRenderPageOk {
    pub entries: Vec<RecentRenderRecord>,
    pub total_count: HistoryRenderCount,
    pub position: HistoryPagePosition,
    pub has_newer: bool,
    pub has_older: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ListRecentRenderPageError {
    #[error(transparent)]
    InvalidRow(#[from] RecentRenderRowError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

pub fn execute(
    query: ListRecentRenderPage,
    connection: &Connection,
) -> Result<ListRecentRenderPageOk, ListRecentRenderPageError> {
    let total_count = connection
        .query_row("SELECT COUNT(*) FROM recent_renders", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(anyhow::Error::from)?;
    let total_count = u64::try_from(total_count).map_err(anyhow::Error::from)?;
    let query_total_count = usize::try_from(total_count).map_err(anyhow::Error::from)?;
    let page_count = query_total_count.div_ceil(RECENT_RENDER_PAGE_SIZE);
    let (entries, has_newer, has_older) = list_page(connection, query.cursor, query_total_count)?;
    let page_number = match query.cursor {
        RecentRenderPageCursor::Newest => usize::from(query_total_count > 0),
        RecentRenderPageCursor::OlderThan { page, .. }
        | RecentRenderPageCursor::NewerThan { page, .. } => usize::try_from(u32::from(page))
            .map_err(anyhow::Error::from)?
            .min(page_count),
        RecentRenderPageCursor::Oldest => page_count,
    };
    let position = history_page_position(page_number, page_count)?;
    Ok(ListRecentRenderPageOk {
        entries,
        total_count: HistoryRenderCount::new(total_count),
        position,
        has_newer,
        has_older,
    })
}

fn history_page_position(
    page_number: usize,
    page_count: usize,
) -> Result<HistoryPagePosition, ListRecentRenderPageError> {
    if page_count == 0 {
        return Ok(HistoryPagePosition::Empty);
    }
    let page_number = u32::try_from(page_number).map_err(anyhow::Error::from)?;
    let page_count = u32::try_from(page_count).map_err(anyhow::Error::from)?;
    let page_number = HistoryPageNumber::try_new(page_number).map_err(anyhow::Error::from)?;
    let page_count = HistoryPageCount::try_new(page_count).map_err(anyhow::Error::from)?;
    let page = HistoryPage::new(page_number, page_count).map_err(anyhow::Error::from)?;
    Ok(HistoryPagePosition::Page(page))
}

fn list_page(
    connection: &Connection,
    cursor: RecentRenderPageCursor,
    total_count: usize,
) -> Result<(Vec<RecentRenderRecord>, bool, bool), ListRecentRenderPageError> {
    let last_page_size = match total_count % RECENT_RENDER_PAGE_SIZE {
        0 => RECENT_RENDER_PAGE_SIZE,
        remainder => remainder,
    };
    let (filter, direction, parameters, ascending, query_size) = match cursor {
        RecentRenderPageCursor::Newest => {
            ("", "DESC", vec![], false, RECENT_RENDER_PAGE_QUERY_SIZE)
        }
        RecentRenderPageCursor::OlderThan { render, .. } => (
            "WHERE r.id < ?1",
            "DESC",
            vec![i64::from(render)],
            false,
            RECENT_RENDER_PAGE_QUERY_SIZE,
        ),
        RecentRenderPageCursor::NewerThan { render, .. } => (
            "WHERE r.id > ?1",
            "ASC",
            vec![i64::from(render)],
            true,
            RECENT_RENDER_PAGE_QUERY_SIZE,
        ),
        RecentRenderPageCursor::Oldest => ("", "ASC", vec![], true, last_page_size),
    };
    let limit_parameter = parameters.len() + 1;
    let sql = format!(
        "{RECENT_RENDER_SELECT} {filter} ORDER BY r.id {direction} LIMIT ?{limit_parameter}"
    );
    let mut parameters = parameters;
    parameters.push(i64::try_from(query_size).map_err(anyhow::Error::from)?);
    let mut statement = connection
        .prepare_cached(&sql)
        .map_err(anyhow::Error::from)?;
    let mut rows = statement
        .query_map(params_from_iter(parameters), RecentRenderRow::from_row)
        .map_err(anyhow::Error::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(anyhow::Error::from)?;
    let has_more = rows.len() > RECENT_RENDER_PAGE_SIZE;
    rows.truncate(RECENT_RENDER_PAGE_SIZE);
    if ascending {
        rows.reverse();
    }
    let entries = rows
        .into_iter()
        .map(RecentRenderRow::try_into_record)
        .collect::<Result<Vec<_>, _>>()?;
    let has_newer = match cursor {
        RecentRenderPageCursor::Newest => false,
        RecentRenderPageCursor::OlderThan { .. } => true,
        RecentRenderPageCursor::NewerThan { .. } => has_more,
        RecentRenderPageCursor::Oldest => total_count > entries.len(),
    };
    let has_older = match cursor {
        RecentRenderPageCursor::Newest | RecentRenderPageCursor::OlderThan { .. } => has_more,
        RecentRenderPageCursor::NewerThan { .. } => true,
        RecentRenderPageCursor::Oldest => false,
    };
    Ok((entries, has_newer, has_older))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{
        list_recent_render_page,
        persistence::{seed_recent_render, store_test},
    };

    fn seed_history(connection: &Connection, count: i64) {
        for id in 1..=count {
            seed_recent_render(connection, id, &format!("render-{id}"));
        }
    }

    fn ids(page: &ListRecentRenderPageOk) -> Vec<i64> {
        page.entries
            .iter()
            .map(|entry| i64::from(entry.id))
            .collect()
    }

    fn page_number(value: u32) -> HistoryPageNumber {
        HistoryPageNumber::try_new(value).expect("fixture page number is positive")
    }

    fn page_position(number: u32, count: u32) -> HistoryPagePosition {
        HistoryPagePosition::Page(
            HistoryPage::new(
                page_number(number),
                HistoryPageCount::try_new(count).expect("fixture page count is positive"),
            )
            .expect("fixture page is within the page count"),
        )
    }

    #[test]
    fn pages_through_history_with_id_keysets() {
        let connection = store_test();
        seed_history(&connection, 65);

        let first = list_recent_render_page::execute(ListRecentRenderPage::default(), &connection)
            .expect("first page");
        assert_eq!(ids(&first), (36..=65).rev().collect::<Vec<_>>());
        assert_eq!(first.position, page_position(1, 3));
        assert_eq!(first.total_count, HistoryRenderCount::new(65));
        assert!(!first.has_newer);
        assert!(first.has_older);

        let second = list_recent_render_page::execute(
            ListRecentRenderPage {
                cursor: RecentRenderPageCursor::OlderThan {
                    render: first.entries.last().expect("first page row").id,
                    page: page_number(2),
                },
            },
            &connection,
        )
        .expect("second page");
        assert_eq!(ids(&second), (6..=35).rev().collect::<Vec<_>>());
        assert!(second.has_newer);
        assert!(second.has_older);

        let last = list_recent_render_page::execute(
            ListRecentRenderPage {
                cursor: RecentRenderPageCursor::Oldest,
            },
            &connection,
        )
        .expect("last page");
        assert_eq!(ids(&last), (1..=5).rev().collect::<Vec<_>>());
        assert_eq!(last.position, page_position(3, 3));
        assert!(last.has_newer);
        assert!(!last.has_older);

        let previous = list_recent_render_page::execute(
            ListRecentRenderPage {
                cursor: RecentRenderPageCursor::NewerThan {
                    render: last.entries.first().expect("last page row").id,
                    page: page_number(2),
                },
            },
            &connection,
        )
        .expect("previous page");
        assert_eq!(ids(&previous), ids(&second));
    }

    #[test]
    fn empty_history_has_no_pages_or_navigation() {
        let connection = store_test();

        let page = list_recent_render_page::execute(ListRecentRenderPage::default(), &connection)
            .expect("empty page");

        assert!(page.entries.is_empty());
        assert_eq!(page.total_count, HistoryRenderCount::default());
        assert_eq!(page.position, HistoryPagePosition::Empty);
        assert!(!page.has_newer);
        assert!(!page.has_older);
    }

    #[test]
    fn keyset_query_uses_the_integer_primary_key_range() {
        let connection = store_test();
        seed_history(&connection, 2);
        let details = connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT id FROM recent_renders \
                 WHERE id < ?1 ORDER BY id DESC LIMIT ?2",
            )
            .expect("prepare query plan")
            .query_map([2_i64, 30_i64], |row| row.get::<_, String>(3))
            .expect("inspect query plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("decode query plan");

        assert!(
            details
                .iter()
                .any(|detail| detail.contains("USING INTEGER PRIMARY KEY (rowid<?)")),
            "{details:?}"
        );
        assert!(!details.iter().any(|detail| detail.contains("TEMP B-TREE")));
    }

    #[test]
    fn invalid_rows_remain_typed_page_errors() {
        let connection = store_test();
        seed_recent_render(&connection, 0, "invalid");

        let error = list_recent_render_page::execute(ListRecentRenderPage::default(), &connection)
            .expect_err("corrupt row identity rejects");

        assert!(matches!(
            error,
            ListRecentRenderPageError::InvalidRow(RecentRenderRowError::Id { id: 0 })
        ));
    }

    #[test]
    fn timezone_less_render_timestamp_remains_a_typed_page_error() {
        let connection = store_test();
        seed_recent_render(&connection, 7, "invalid timestamp");
        connection
            .execute(
                "UPDATE recent_renders SET rendered_at = '2026-07-11T00:00:00' WHERE id = 7",
                [],
            )
            .expect("corrupt persisted timestamp");

        let error = list_recent_render_page::execute(ListRecentRenderPage::default(), &connection)
            .expect_err("timezone-less timestamp must reject");

        assert!(matches!(
            error,
            ListRecentRenderPageError::InvalidRow(RecentRenderRowError::Timestamp { id: 7, .. })
        ));
    }
}
