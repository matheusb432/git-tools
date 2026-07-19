use domain::viewer::RenderHistoryId;

#[cfg(test)]
use crate::testing::AppStateStoreTest;

/// One persisted render recipe with its stable row identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRenderRecord {
    pub id: RenderHistoryId,
    pub recipe_json: String,
    pub title: String,
    pub repo_name: String,
    pub kind: String,
    pub range_label: String,
    pub rendered_at: String,
}

pub(super) struct RecentRenderRow {
    id: i64,
    recipe_json: String,
    title: String,
    repo_name: String,
    kind: String,
    range_label: String,
    rendered_at: String,
}

impl RecentRenderRow {
    pub(super) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            recipe_json: row.get(1)?,
            title: row.get(2)?,
            repo_name: row.get(3)?,
            kind: row.get(4)?,
            range_label: row.get(5)?,
            rendered_at: row.get(6)?,
        })
    }

    pub(super) fn try_into_record(self) -> Result<RecentRenderRecord, RecentRenderIdentityError> {
        let id = RenderHistoryId::try_new(self.id)
            .map_err(|_| RecentRenderIdentityError { id: self.id })?;
        Ok(RecentRenderRecord {
            id,
            recipe_json: self.recipe_json,
            title: self.title,
            repo_name: self.repo_name,
            kind: self.kind,
            range_label: self.range_label,
            rendered_at: self.rendered_at,
        })
    }
}

#[derive(Debug)]
pub(super) struct RecentRenderIdentityError {
    pub(super) id: i64,
}

#[cfg(test)]
pub(crate) fn store_test() -> AppStateStoreTest {
    AppStateStoreTest::new(
        "CREATE TABLE recent_renders (
          id          INTEGER PRIMARY KEY,
          recipe_json TEXT NOT NULL,
          title       TEXT NOT NULL,
          repo_name   TEXT NOT NULL,
          kind        TEXT NOT NULL,
          range_label TEXT NOT NULL,
          rendered_at TEXT NOT NULL
        ) STRICT;",
    )
    .expect("history test store")
}
