//! The keyset-paginated query for the desktop viewer's recent render history.

use std::num::NonZeroUsize;

use gtl_models::viewer::RenderHistoryId;
use rusqlite::{Connection, params_from_iter};

use crate::history::{
    RecentRenderRecord, RecentRenderRowError,
    logic::persistence::{RECENT_RENDER_SELECT, RecentRenderRow},
};

pub const RECENT_RENDER_PAGE_SIZE: usize = 30;
const RECENT_RENDER_PAGE_QUERY_SIZE: usize = RECENT_RENDER_PAGE_SIZE + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecentRenderPageCursor {
    #[default]
    Newest,
    OlderThan {
        render: RenderHistoryId,
        page: NonZeroUsize,
    },
    NewerThan {
        render: RenderHistoryId,
        page: NonZeroUsize,
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
    pub total_count: usize,
    pub page_number: usize,
    pub page_count: usize,
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
    let total_count = usize::try_from(total_count).map_err(anyhow::Error::from)?;
    let page_count = total_count.div_ceil(RECENT_RENDER_PAGE_SIZE);
    let (entries, has_newer, has_older) = list_page(connection, query.cursor, total_count)?;
    let page_number = match query.cursor {
        RecentRenderPageCursor::Newest => usize::from(total_count > 0),
        RecentRenderPageCursor::OlderThan { page, .. }
        | RecentRenderPageCursor::NewerThan { page, .. } => page.get().min(page_count),
        RecentRenderPageCursor::Oldest => page_count,
    };
    Ok(ListRecentRenderPageOk {
        entries,
        total_count,
        page_number,
        page_count,
        has_newer,
        has_older,
    })
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
    use crate::history::logic::persistence::{seed_recent_render, store_test};

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

    #[test]
    fn pages_through_history_with_id_keysets() {
        let connection = store_test();
        seed_history(&connection, 65);

        let first = execute(ListRecentRenderPage::default(), &connection).expect("first page");
        assert_eq!(ids(&first), (36..=65).rev().collect::<Vec<_>>());
        assert_eq!(
            (first.page_number, first.page_count, first.total_count),
            (1, 3, 65)
        );
        assert!(!first.has_newer);
        assert!(first.has_older);

        let second = execute(
            ListRecentRenderPage {
                cursor: RecentRenderPageCursor::OlderThan {
                    render: first.entries.last().expect("first page row").id,
                    page: NonZeroUsize::new(2).expect("positive page"),
                },
            },
            &connection,
        )
        .expect("second page");
        assert_eq!(ids(&second), (6..=35).rev().collect::<Vec<_>>());
        assert!(second.has_newer);
        assert!(second.has_older);

        let last = execute(
            ListRecentRenderPage {
                cursor: RecentRenderPageCursor::Oldest,
            },
            &connection,
        )
        .expect("last page");
        assert_eq!(ids(&last), (1..=5).rev().collect::<Vec<_>>());
        assert_eq!(last.page_number, 3);
        assert!(last.has_newer);
        assert!(!last.has_older);

        let previous = execute(
            ListRecentRenderPage {
                cursor: RecentRenderPageCursor::NewerThan {
                    render: last.entries.first().expect("last page row").id,
                    page: NonZeroUsize::new(2).expect("positive page"),
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

        let page = execute(ListRecentRenderPage::default(), &connection).expect("empty page");

        assert!(page.entries.is_empty());
        assert_eq!(
            (page.total_count, page.page_number, page.page_count),
            (0, 0, 0)
        );
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

        let error = execute(ListRecentRenderPage::default(), &connection)
            .expect_err("corrupt row identity rejects");

        assert!(matches!(
            error,
            ListRecentRenderPageError::InvalidRow(RecentRenderRowError::InvalidId { id: 0 })
        ));
    }
}
