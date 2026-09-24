use std::path::PathBuf;

use anyhow::Context as _;
use gtl_models::{failure::ErrorMeta, live_views::LiveSource};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior, params};

use crate::{
    live_views::LiveViewRecord,
    ports::{Clock, GitClient, GitRepositoryState},
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SaveLiveView {
    pub path: PathBuf,
    pub comparison: gtl_models::live_views::LiveComparison,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveLiveViewOk {
    pub outcome: SaveLiveViewOutcome,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SaveLiveViewOutcome {
    Created { record: LiveViewRecord },
    Refreshed { record: LiveViewRecord },
    Rejected { rejection: LiveViewRejection },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LiveViewRejection {
    #[error("The git repo's directory at `{path}` was not found.")]
    DirNotFound { path: String },
    #[error("The directory `{path}` is not a git repository.")]
    DirNotGitRepo { path: String },
}

impl LiveViewRejection {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::DirNotFound { .. } => "DirNotFound",
            Self::DirNotGitRepo { .. } => "DirNotGitRepo",
        }
    }

    /// The typed reason a live view built from this rejection reports.
    #[must_use]
    pub fn failure(&self) -> gtl_models::failure::ViewerFailure {
        use gtl_models::failure::ViewerFailure;
        match self {
            Self::DirNotFound { path } => {
                ViewerFailure::SourceDirectoryMissing { path: path.into() }
            }
            Self::DirNotGitRepo { path } => {
                ViewerFailure::SourceNotRepository { path: path.into() }
            }
        }
    }
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SaveLiveViewError {
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

pub fn execute(
    req: SaveLiveView,
    git: &impl GitClient,
    connection: &mut Connection,
    clock: &impl Clock,
) -> Result<SaveLiveViewOk, SaveLiveViewError> {
    let SaveLiveView { path, comparison } = req;
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
    let display_name = source.display_name();
    let record = LiveViewRecord {
        source,
        comparison,
        display_name,
        created_at: clock.now().map_err(anyhow::Error::from)?,
        last_opened_at: None,
    };
    let (outcome, text) = match save_live_view(connection, record)? {
        StoredLiveView::Created(record) => {
            let text = format!("saved live view for `{}`", record.display_name);
            (SaveLiveViewOutcome::Created { record }, text)
        }
        StoredLiveView::Refreshed(record) => {
            let text = format!(
                "live view for `{}` already saved; refreshed",
                record.display_name
            );
            (SaveLiveViewOutcome::Refreshed { record }, text)
        }
    };
    Ok(SaveLiveViewOk {
        outcome,
        notes: vec![Note::info(text)],
    })
}

enum StoredLiveView {
    Created(LiveViewRecord),
    Refreshed(LiveViewRecord),
}

fn save_live_view(
    connection: &mut Connection,
    record: LiveViewRecord,
) -> anyhow::Result<StoredLiveView> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let source_value = record.source.value();
    let raw_previous_times = {
        let mut statement = transaction.prepare_cached(
            "SELECT created_at, last_opened_at
             FROM live_views WHERE source_kind = ?1 AND source_value = ?2 AND comparison = ?3",
        )?;
        statement
            .query_row(
                params![
                    record.source.kind(),
                    &source_value,
                    record.comparison.as_str()
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?
    };
    let previous_times = raw_previous_times
        .map(|(created_at, last_opened_at)| {
            let created_at = created_at
                .try_into()
                .context("persisted live view has an invalid creation timestamp")?;
            let last_opened_at = last_opened_at
                .map(TryInto::try_into)
                .transpose()
                .context("persisted live view has an invalid last-opened timestamp")?;
            Ok::<_, anyhow::Error>((created_at, last_opened_at))
        })
        .transpose()?;
    {
        let mut statement = transaction.prepare_cached(
            "INSERT INTO live_views (source_kind, source_value, display_name, created_at, last_opened_at, comparison)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(source_kind, source_value, comparison) DO UPDATE SET display_name = excluded.display_name",
        )?;
        statement.execute(params![
            record.source.kind(),
            &source_value,
            record.display_name.as_str(),
            record.created_at.as_ref(),
            record
                .last_opened_at
                .as_ref()
                .map(gtl_models::timestamps::MachineTimestamp::as_ref),
            record.comparison.as_str(),
        ])?;
    }
    transaction.commit()?;
    Ok(match previous_times {
        None => StoredLiveView::Created(record),
        Some((created_at, last_opened_at)) => StoredLiveView::Refreshed(LiveViewRecord {
            created_at,
            last_opened_at,
            ..record
        }),
    })
}

fn rejected(rejection: LiveViewRejection) -> SaveLiveViewOk {
    SaveLiveViewOk {
        notes: vec![Note::warn(rejection.to_string())],
        outcome: SaveLiveViewOutcome::Rejected { rejection },
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::{
        live_views::{list_live_views, persistence::store_test, save_live_view},
        ports::GitRepositoryState,
        shared::notes::NoteLevel,
        utils::{FakeGitClient, FixedClock},
    };

    fn dependencies(
        repository_state: GitRepositoryState,
    ) -> (FakeGitClient, Connection, FixedClock) {
        (
            FakeGitClient {
                repository_state: Some(repository_state),
                ..Default::default()
            },
            store_test(),
            FixedClock::from_raw("2026-01-01T00:00:00Z"),
        )
    }

    fn list_views(connection: &Connection) -> Vec<LiveViewRecord> {
        list_live_views::execute(list_live_views::ListLiveViews, connection).unwrap()
    }

    fn created_at(raw: &str) -> gtl_models::timestamps::MachineTimestamp {
        raw.try_into().unwrap()
    }

    fn opened_at(raw: &str) -> gtl_models::timestamps::MachineTimestamp {
        raw.try_into().unwrap()
    }

    #[test]
    fn missing_dir_rejects_with_dir_not_found() {
        let (git, mut connection, clock) = dependencies(GitRepositoryState::NotFound);
        let response = save_live_view::execute(
            SaveLiveView {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                path: "/gone".into(),
            },
            &git,
            &mut connection,
            &clock,
        )
        .unwrap();

        let rejection = match &response.outcome {
            SaveLiveViewOutcome::Rejected { rejection } => Some(rejection),
            SaveLiveViewOutcome::Created { .. } | SaveLiveViewOutcome::Refreshed { .. } => None,
        }
        .unwrap();
        assert_eq!(rejection.code(), "DirNotFound");
        assert_eq!(
            rejection.to_string(),
            "The git repo's directory at `/gone` was not found."
        );
        assert_eq!(response.notes.len(), 1);
        assert_eq!(response.notes[0].level, NoteLevel::Warn);
        assert_eq!(
            response.notes[0].text,
            "The git repo's directory at `/gone` was not found."
        );
        assert!(list_views(&connection).is_empty());
    }

    #[test]
    fn non_repo_dir_rejects_with_dir_not_git_repo() {
        let (git, mut connection, clock) = dependencies(GitRepositoryState::NotARepository);
        let response = save_live_view::execute(
            SaveLiveView {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                path: "/plain".into(),
            },
            &git,
            &mut connection,
            &clock,
        )
        .unwrap();

        let rejection = match &response.outcome {
            SaveLiveViewOutcome::Rejected { rejection } => Some(rejection),
            SaveLiveViewOutcome::Created { .. } | SaveLiveViewOutcome::Refreshed { .. } => None,
        }
        .unwrap();
        assert_eq!(rejection.code(), "DirNotGitRepo");
        assert_eq!(
            rejection.to_string(),
            "The directory `/plain` is not a git repository."
        );
        assert_eq!(
            response.notes[0].text,
            "The directory `/plain` is not a git repository."
        );
        assert!(list_views(&connection).is_empty());
    }

    #[test]
    fn valid_repo_saves_a_record_with_canonical_identity_and_clock_time() {
        let (git, mut connection, clock) = dependencies(GitRepositoryState::Repository {
            top_level: crate::utils::repository_root("/repos/gt"),
        });
        let response = save_live_view::execute(
            SaveLiveView {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                path: "/repos/gt".into(),
            },
            &git,
            &mut connection,
            &clock,
        )
        .unwrap();

        let record = match &response.outcome {
            SaveLiveViewOutcome::Created { record } => Some(record),
            SaveLiveViewOutcome::Refreshed { .. } | SaveLiveViewOutcome::Rejected { .. } => None,
        }
        .unwrap();
        assert_eq!(
            record,
            &LiveViewRecord {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                source: LiveSource::local_repo(crate::utils::repository_root("/repos/gt")),
                display_name: crate::utils::project_name("gt"),
                created_at: created_at("2026-01-01T00:00:00Z"),
                last_opened_at: None,
            }
        );
        assert_eq!(response.notes.len(), 1);
        assert_eq!(response.notes[0].level, NoteLevel::Info);
        assert_eq!(response.notes[0].text, "saved live view for `gt`");
        assert_eq!(list_views(&connection).len(), 1);
    }

    #[test]
    fn resaving_reports_refresh_and_returns_the_persisted_timestamps() {
        let (git, mut connection, clock) = dependencies(GitRepositoryState::Repository {
            top_level: crate::utils::repository_root("/repos/gt"),
        });
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at, last_opened_at) \
                 VALUES ('LocalRepo', '/repos/gt', 'old name', \
                 '2025-01-01T00:00:00Z', '2025-06-01T00:00:00Z')",
                [],
            )
            .unwrap();

        let response = save_live_view::execute(
            SaveLiveView {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                path: "/repos/gt".into(),
            },
            &git,
            &mut connection,
            &clock,
        )
        .unwrap();

        let record = match &response.outcome {
            SaveLiveViewOutcome::Refreshed { record } => Some(record),
            SaveLiveViewOutcome::Created { .. } | SaveLiveViewOutcome::Rejected { .. } => None,
        }
        .unwrap();
        assert_eq!(record.created_at.as_ref(), "2025-01-01T00:00:00Z");
        assert_eq!(
            record.last_opened_at.as_ref().map(AsRef::as_ref),
            Some("2025-06-01T00:00:00Z")
        );
        assert_eq!(
            response.notes[0].text,
            "live view for `gt` already saved; refreshed"
        );
        assert_eq!(
            list_views(&connection),
            vec![LiveViewRecord {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                source: LiveSource::local_repo(crate::utils::repository_root("/repos/gt")),
                display_name: crate::utils::project_name("gt"),
                created_at: created_at("2025-01-01T00:00:00Z"),
                last_opened_at: Some(opened_at("2025-06-01T00:00:00Z")),
            }]
        );
    }

    #[test]
    fn resaving_rejects_a_malformed_persisted_timestamp_before_mutation() {
        let (git, mut connection, clock) = dependencies(GitRepositoryState::Repository {
            top_level: crate::utils::repository_root("/repos/gt"),
        });
        connection
            .execute(
                "INSERT INTO live_views \
                 (source_kind, source_value, display_name, created_at) \
                 VALUES ('LocalRepo', '/repos/gt', 'old name', '2025-01-01T00:00:00')",
                [],
            )
            .unwrap();

        save_live_view::execute(
            SaveLiveView {
                comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                path: "/repos/gt".into(),
            },
            &git,
            &mut connection,
            &clock,
        )
        .unwrap_err();

        let display_name = connection
            .query_row(
                "SELECT display_name FROM live_views WHERE source_value = '/repos/gt'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(display_name, "old name");
    }
}
