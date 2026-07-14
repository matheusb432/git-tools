//! The `history/record_render` vertical slice: record one render in the app history log.

use std::path::PathBuf;

use crate::ports::{AppStateStore, Clock, NewRecentRenderRecord};

/// Record one render in the app history.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordRender {
    pub data_root: PathBuf,
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
#[cqrsy::handler(command)]
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
    store.record_render(&req.data_root, &record)?;
    Ok(RecordRenderResponse {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FixedClock, InMemoryAppStateStore};

    #[test]
    fn records_a_render_stamped_by_the_clock() {
        let store = InMemoryAppStateStore::default();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        execute(
            RecordRender {
                data_root: "/data".into(),
                recipe_json: r#"{"kind":"diff"}"#.into(),
                title: "gt · unpushed".into(),
                repo_name: "gt".into(),
                kind: "diff".into(),
                range_label: "origin/main..HEAD".into(),
            },
            &store,
            &clock,
        )
        .expect("record succeeds");

        let renders = store.renders.lock().unwrap();
        assert_eq!(renders.len(), 1);
        assert_eq!(
            renders[0],
            crate::ports::RecentRenderRecord {
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
}
