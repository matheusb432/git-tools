//! The `history/record_render` vertical slice: record one render in the app history log.

use rusqlite::{Connection, params};

use crate::ports::{AppStateStore, Clock};

const RECENT_RENDERS_CAP: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
struct NewRecentRenderRecord {
    recipe_json: String,
    title: String,
    repo_name: String,
    kind: String,
    range_label: String,
    rendered_at: String,
}

/// Record one render in the app history.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordRender {
    pub recipe_json: String,
    pub title: String,
    pub repo_name: String,
    pub kind: String,
    pub range_label: String,
}

/// Response when a render is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRenderResponse {}

/// Error when recording a render fails.
#[derive(Debug, thiserror::Error)]
pub enum RecordRenderError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Records a render through the app-state port.
#[cqrsy::command]
pub fn execute(
    req: RecordRender,
    store: &impl AppStateStore,
    clock: &impl Clock,
) -> Result<RecordRenderResponse, RecordRenderError> {
    let record = NewRecentRenderRecord {
        recipe_json: req.recipe_json,
        title: req.title,
        repo_name: req.repo_name,
        kind: req.kind,
        range_label: req.range_label,
        rendered_at: clock.now_iso(),
    };
    let mut connection = store.connection_lock()?;
    record_render(&mut connection, &record)?;
    Ok(RecordRenderResponse {})
}

fn record_render(
    connection: &mut Connection,
    record: &NewRecentRenderRecord,
) -> anyhow::Result<()> {
    let transaction = connection.transaction()?;
    {
        let mut statement = transaction.prepare_cached(
            "INSERT INTO recent_renders \
             (recipe_json, title, repo_name, kind, range_label, rendered_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        statement.execute(params![
            record.recipe_json,
            record.title,
            record.repo_name,
            record.kind,
            record.range_label,
            record.rendered_at,
        ])?;
    }
    {
        let mut statement = transaction.prepare_cached(
            "DELETE FROM recent_renders WHERE id NOT IN
             (SELECT id FROM recent_renders ORDER BY id DESC LIMIT ?1)",
        )?;
        statement.execute(params![i64::try_from(RECENT_RENDERS_CAP)?])?;
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::{RecentRenderRecord, list_recent, persistence::store_test},
        testing::FixedClock,
    };

    fn command(title: impl Into<String>) -> RecordRender {
        RecordRender {
            recipe_json: r#"{"kind":"diff"}"#.into(),
            title: title.into(),
            repo_name: "gt".into(),
            kind: "diff".into(),
            range_label: "origin/main..HEAD".into(),
        }
    }

    fn list_recent(store: &impl AppStateStore) -> Vec<RecentRenderRecord> {
        list_recent::list::execute(list_recent::ListRecentRenders, store)
            .expect("list succeeds")
            .entries
    }

    #[test]
    fn records_a_render_stamped_by_the_clock() {
        let store = store_test();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        execute(command("gt · unpushed"), &store, &clock).expect("record succeeds");

        let renders = list_recent(&store);
        assert_eq!(renders.len(), 1);
        assert_eq!(
            renders[0],
            RecentRenderRecord {
                id: domain::viewer::RenderHistoryId::try_new(1).expect("positive id"),
                recipe_json: r#"{"kind":"diff"}"#.into(),
                title: "gt · unpushed".into(),
                repo_name: "gt".into(),
                kind: "diff".into(),
                range_label: "origin/main..HEAD".into(),
                rendered_at: "2026-07-07T00:00:00Z".into(),
            }
        );
    }

    #[test]
    fn recording_past_the_cap_prunes_oldest_rows() {
        let store = store_test();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());

        for index in 0..(RECENT_RENDERS_CAP + 5) {
            execute(command(format!("render {index}")), &store, &clock).expect("record succeeds");
        }

        let renders = list_recent(&store);
        assert_eq!(renders.len(), RECENT_RENDERS_CAP);
        assert_eq!(renders[0].title, "render 504");
        assert_eq!(renders[RECENT_RENDERS_CAP - 1].title, "render 5");
    }
}
