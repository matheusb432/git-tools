//! The `live_views/save` vertical slice: validate a directory as a git repo
//! and persist (or refresh) a saved live view for it.

use std::path::PathBuf;

use domain::live_views::LiveSource;
use rusqlite::{Connection, TransactionBehavior, params};

use crate::{
    live_views::LiveViewRecord,
    ports::{AppStateStore, Clock, GitClient, GitRepositoryState},
    shared::notes::Note,
};

/// Validate `path` as a git repo and save (or refresh) a live view for it.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveLiveView {
    pub path: PathBuf,
}

/// The outcome plus every message the save wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveLiveViewOk {
    pub outcome: SaveLiveViewOutcome,
    pub notes: Vec<Note>,
}

/// What a save produced: a persisted record, or a rejected candidate directory.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveLiveViewOutcome {
    /// The record now lives in the store; `already_saved` when it was a refresh.
    Saved {
        record: LiveViewRecord,
        already_saved: bool,
    },
    /// `path` did not validate as a saveable git repo.
    Rejected { rejection: LiveViewRejection },
}

/// Why a live-view save was rejected — typed codes with the service-owned messages.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LiveViewRejection {
    #[error("The git repo's directory at `{path}` was not found.")]
    DirNotFound { path: String },
    #[error("The directory `{path}` is not a git repository.")]
    DirNotGitRepo { path: String },
}

impl LiveViewRejection {
    /// The stable machine-readable code for this rejection.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DirNotFound { .. } => "DirNotFound",
            Self::DirNotGitRepo { .. } => "DirNotGitRepo",
        }
    }
}

/// Everything that can go wrong saving a live view.
#[derive(Debug, thiserror::Error)]
pub enum SaveLiveViewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Saves a live view by probing `path` and writing through the app-state port.
#[cqrsy::command]
pub fn execute(
    req: SaveLiveView,
    git: &impl GitClient,
    store: &impl AppStateStore,
    clock: &impl Clock,
) -> Result<SaveLiveViewOk, SaveLiveViewError> {
    let SaveLiveView { path } = req;
    let top_level = match git.probe_repository(&path)? {
        GitRepositoryState::Repository { top_level } => top_level,
        GitRepositoryState::NotFound => {
            return Ok(rejected(LiveViewRejection::DirNotFound {
                path: path.display().to_string(),
            }));
        }
        GitRepositoryState::NotARepository => {
            return Ok(rejected(LiveViewRejection::DirNotGitRepo {
                path: path.display().to_string(),
            }));
        }
    };

    let source = LiveSource::local_repo(top_level);
    let record = LiveViewRecord {
        source_kind: source.kind().to_string(),
        source_value: source.value(),
        display_name: source.display_name(),
        created_at: clock.now_iso(),
        last_opened_at: None,
    };
    let mut connection = store.connection_lock()?;
    let already_saved = save_live_view(&mut connection, &record)?;
    let text = if already_saved {
        format!(
            "live view for `{}` already saved — refreshed",
            record.display_name
        )
    } else {
        format!("saved live view for `{}`", record.display_name)
    };
    Ok(SaveLiveViewOk {
        outcome: SaveLiveViewOutcome::Saved {
            record,
            already_saved,
        },
        notes: vec![Note::info(text)],
    })
}

fn save_live_view(connection: &mut Connection, record: &LiveViewRecord) -> anyhow::Result<bool> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existed = {
        let mut statement = transaction.prepare_cached(
            "SELECT EXISTS(
               SELECT 1 FROM live_views WHERE source_kind = ?1 AND source_value = ?2
             )",
        )?;
        statement.query_row(params![record.source_kind, record.source_value], |row| {
            row.get::<_, bool>(0)
        })?
    };
    {
        let mut statement = transaction.prepare_cached(
            "INSERT INTO live_views (source_kind, source_value, display_name, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_kind, source_value) DO UPDATE SET display_name = excluded.display_name",
        )?;
        statement.execute(params![
            record.source_kind,
            record.source_value,
            record.display_name,
            record.created_at,
            record.last_opened_at,
        ])?;
    }
    transaction.commit()?;
    Ok(existed)
}

/// Builds the rejected response: an [`SaveLiveViewOutcome::Rejected`] outcome
/// carrying the rejection's `Display` text as a warn-level note.
fn rejected(rejection: LiveViewRejection) -> SaveLiveViewOk {
    SaveLiveViewOk {
        notes: vec![Note::warn(rejection.to_string())],
        outcome: SaveLiveViewOutcome::Rejected { rejection },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        live_views::{list, persistence::store_test},
        ports::{AppStateStore, GitRepositoryState},
        shared::notes::NoteLevel,
        testing::{AppStateStoreTest, FakeGitClient, FixedClock},
    };

    fn dependencies(
        repository_state: GitRepositoryState,
    ) -> (FakeGitClient, AppStateStoreTest, FixedClock) {
        (
            FakeGitClient {
                repository_state: Some(repository_state),
                ..Default::default()
            },
            store_test(),
            FixedClock("2026-01-01T00:00:00Z".into()),
        )
    }

    fn list_views(store: &AppStateStoreTest) -> Vec<LiveViewRecord> {
        list::execute(list::ListLiveViews, store)
            .expect("list succeeds")
            .views
    }

    #[test]
    fn missing_dir_rejects_with_dir_not_found() {
        let (git, store, clock) = dependencies(GitRepositoryState::NotFound);
        let response = execute(
            SaveLiveView {
                path: "/gone".into(),
            },
            &git,
            &store,
            &clock,
        )
        .expect("save succeeds with a rejected outcome");

        match &response.outcome {
            SaveLiveViewOutcome::Rejected { rejection } => {
                assert_eq!(rejection.code(), "DirNotFound");
                assert_eq!(
                    rejection.to_string(),
                    "The git repo's directory at `/gone` was not found."
                );
            }
            SaveLiveViewOutcome::Saved { .. } => panic!("expected Rejected, got Saved"),
        }
        assert_eq!(response.notes.len(), 1);
        assert_eq!(response.notes[0].level, NoteLevel::Warn);
        assert_eq!(
            response.notes[0].text,
            "The git repo's directory at `/gone` was not found."
        );
        assert!(list_views(&store).is_empty());
    }

    #[test]
    fn non_repo_dir_rejects_with_dir_not_git_repo() {
        let (git, store, clock) = dependencies(GitRepositoryState::NotARepository);
        let response = execute(
            SaveLiveView {
                path: "/plain".into(),
            },
            &git,
            &store,
            &clock,
        )
        .expect("save succeeds with a rejected outcome");

        match &response.outcome {
            SaveLiveViewOutcome::Rejected { rejection } => {
                assert_eq!(rejection.code(), "DirNotGitRepo");
                assert_eq!(
                    rejection.to_string(),
                    "The directory `/plain` is not a git repository."
                );
            }
            SaveLiveViewOutcome::Saved { .. } => panic!("expected Rejected, got Saved"),
        }
        assert_eq!(
            response.notes[0].text,
            "The directory `/plain` is not a git repository."
        );
        assert!(list_views(&store).is_empty());
    }

    #[test]
    fn valid_repo_saves_a_record_with_canonical_identity_and_clock_time() {
        let (git, store, clock) = dependencies(GitRepositoryState::Repository {
            top_level: "/repos/gt".into(),
        });
        let response = execute(
            SaveLiveView {
                path: "/repos/gt".into(),
            },
            &git,
            &store,
            &clock,
        )
        .expect("save succeeds");

        match &response.outcome {
            SaveLiveViewOutcome::Saved {
                record,
                already_saved,
            } => {
                assert!(!already_saved);
                assert_eq!(
                    record,
                    &LiveViewRecord {
                        source_kind: "LocalRepo".into(),
                        source_value: "/repos/gt".into(),
                        display_name: "gt".into(),
                        created_at: "2026-01-01T00:00:00Z".into(),
                        last_opened_at: None,
                    }
                );
            }
            SaveLiveViewOutcome::Rejected { .. } => panic!("expected Saved, got Rejected"),
        }
        assert_eq!(response.notes.len(), 1);
        assert_eq!(response.notes[0].level, NoteLevel::Info);
        assert_eq!(response.notes[0].text, "saved live view for `gt`");
        assert_eq!(list_views(&store).len(), 1);
    }

    #[test]
    fn resaving_reports_already_saved_and_updates_only_display_name() {
        let (git, store, clock) = dependencies(GitRepositoryState::Repository {
            top_level: "/repos/gt".into(),
        });
        store
            .connection_lock()
            .expect("connection lock")
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at, last_opened_at) \
                 VALUES ('LocalRepo', '/repos/gt', 'old name', \
                 '2025-01-01T00:00:00Z', '2025-06-01T00:00:00Z')",
                [],
            )
            .expect("seed live view");

        let response = execute(
            SaveLiveView {
                path: "/repos/gt".into(),
            },
            &git,
            &store,
            &clock,
        )
        .expect("save succeeds");

        match &response.outcome {
            SaveLiveViewOutcome::Saved { already_saved, .. } => assert!(already_saved),
            SaveLiveViewOutcome::Rejected { .. } => panic!("expected Saved, got Rejected"),
        }
        assert_eq!(
            response.notes[0].text,
            "live view for `gt` already saved — refreshed"
        );
        assert_eq!(
            list_views(&store),
            vec![LiveViewRecord {
                source_kind: "LocalRepo".into(),
                source_value: "/repos/gt".into(),
                display_name: "gt".into(),
                created_at: "2025-01-01T00:00:00Z".into(),
                last_opened_at: Some("2025-06-01T00:00:00Z".into()),
            }]
        );
    }
}
