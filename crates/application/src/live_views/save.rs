//! The `live_views/save` vertical slice: validate a directory as a git repo
//! and persist (or refresh) a saved live view for it.

use std::path::PathBuf;

use cqrsy::Handler;
use domain::live_views::LiveSource;

use crate::{
    ports::{AppStateStore, Clock, LiveViewRecord, RepoProbe, RepoProbeResult},
    shared::notes::Note,
};

/// Validate `path` as a git repo and save (or refresh) a live view for it.
#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = SaveLiveViewResponse, err = SaveLiveViewError)]
pub struct SaveLiveView {
    pub data_root: PathBuf,
    pub path: PathBuf,
}

/// The outcome plus every message the save wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveLiveViewResponse {
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

/// Handles [`SaveLiveView`] by probing `path` and, when it validates, writing
/// through the app-state port.
#[derive(Clone)]
pub struct SaveLiveViewHandler<P: RepoProbe, A: AppStateStore, C: Clock> {
    pub probe: P,
    pub store: A,
    pub clock: C,
}

impl<P: RepoProbe, A: AppStateStore, C: Clock> Handler<SaveLiveView>
    for SaveLiveViewHandler<P, A, C>
{
    async fn handle(&self, req: SaveLiveView) -> Result<SaveLiveViewResponse, SaveLiveViewError> {
        let top_level = match self.probe.probe(&req.path)? {
            RepoProbeResult::Repo { top_level } => top_level,
            RepoProbeResult::NotFound => {
                return Ok(rejected(LiveViewRejection::DirNotFound {
                    path: req.path.display().to_string(),
                }));
            }
            RepoProbeResult::NotAGitRepo => {
                return Ok(rejected(LiveViewRejection::DirNotGitRepo {
                    path: req.path.display().to_string(),
                }));
            }
        };

        let source = LiveSource::local_repo(top_level);
        let record = LiveViewRecord {
            source_kind: source.kind().to_string(),
            source_value: source.value(),
            display_name: source.display_name(),
            created_at: self.clock.now_iso(),
            last_opened_at: None,
        };
        let already_saved = self.store.save_live_view(&req.data_root, &record)?;
        let text = if already_saved {
            format!(
                "live view for `{}` already saved — refreshed",
                record.display_name
            )
        } else {
            format!("saved live view for `{}`", record.display_name)
        };
        Ok(SaveLiveViewResponse {
            outcome: SaveLiveViewOutcome::Saved {
                record,
                already_saved,
            },
            notes: vec![Note::info(text)],
        })
    }
}

/// Builds the rejected response: an [`SaveLiveViewOutcome::Rejected`] outcome
/// carrying the rejection's `Display` text as a warn-level note.
fn rejected(rejection: LiveViewRejection) -> SaveLiveViewResponse {
    SaveLiveViewResponse {
        notes: vec![Note::warn(rejection.to_string())],
        outcome: SaveLiveViewOutcome::Rejected { rejection },
    }
}

#[cfg(test)]
mod tests {
    use cqrsy::send_now;

    use super::*;
    use crate::{
        ports::{LiveViewRecord, RepoProbeResult},
        shared::notes::NoteLevel,
        testing::{FakeRepoProbe, FixedClock, InMemoryAppStateStore},
    };

    fn handler(
        probe_result: RepoProbeResult,
    ) -> SaveLiveViewHandler<FakeRepoProbe, InMemoryAppStateStore, FixedClock> {
        SaveLiveViewHandler {
            probe: FakeRepoProbe {
                result: probe_result,
            },
            store: InMemoryAppStateStore::default(),
            clock: FixedClock("2026-01-01T00:00:00Z".into()),
        }
    }

    #[test]
    fn missing_dir_rejects_with_dir_not_found() {
        let handler = handler(RepoProbeResult::NotFound);

        let response = send_now(
            &(),
            &handler,
            SaveLiveView {
                data_root: "/data".into(),
                path: "/gone".into(),
            },
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
        assert!(
            handler.store.live_views.lock().unwrap().is_empty(),
            "nothing should be persisted on rejection"
        );
    }

    #[test]
    fn non_repo_dir_rejects_with_dir_not_git_repo() {
        let handler = handler(RepoProbeResult::NotAGitRepo);

        let response = send_now(
            &(),
            &handler,
            SaveLiveView {
                data_root: "/data".into(),
                path: "/plain".into(),
            },
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
        assert!(
            handler.store.live_views.lock().unwrap().is_empty(),
            "nothing should be persisted on rejection"
        );
    }

    #[test]
    fn valid_repo_saves_a_record_with_canonical_identity_and_clock_time() {
        let handler = handler(RepoProbeResult::Repo {
            top_level: "/repos/gt".into(),
        });

        let response = send_now(
            &(),
            &handler,
            SaveLiveView {
                data_root: "/data".into(),
                path: "/repos/gt".into(),
            },
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
        assert_eq!(handler.store.live_views.lock().unwrap().len(), 1);
    }

    #[test]
    fn resaving_reports_already_saved() {
        let handler = handler(RepoProbeResult::Repo {
            top_level: "/repos/gt".into(),
        });
        handler
            .store
            .live_views
            .lock()
            .unwrap()
            .push(LiveViewRecord {
                source_kind: "LocalRepo".into(),
                source_value: "/repos/gt".into(),
                display_name: "gt".into(),
                created_at: "2025-01-01T00:00:00Z".into(),
                last_opened_at: None,
            });

        let response = send_now(
            &(),
            &handler,
            SaveLiveView {
                data_root: "/data".into(),
                path: "/repos/gt".into(),
            },
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
        assert_eq!(handler.store.live_views.lock().unwrap().len(), 1);
    }
}
