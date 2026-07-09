//! Live-view and settings commands: `SQLite`-backed app state through the
//! mediator's application slices. Rejections cross as typed codes plus the
//! service-owned reason text, so the frontend can map codes to UI states.

use std::path::Path;

use application::{
    live_views::{
        list::ListLiveViews,
        probe::{ProbeOutcome, ProbeSource},
        remove::RemoveLiveView,
        save::{SaveLiveView, SaveLiveViewOutcome},
    },
    ports::LiveViewRecord,
    settings::{get::GetSetting, set::SetSetting},
};
use cqrsy::{Handle, Sender};
use serde::Serialize;

/// One saved live view row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LiveViewDto {
    pub source_kind: String,
    pub source_value: String,
    pub display_name: String,
    pub created_at: String,
    pub last_opened_at: Option<String>,
}

impl From<LiveViewRecord> for LiveViewDto {
    fn from(record: LiveViewRecord) -> Self {
        Self {
            source_kind: record.source_kind,
            source_value: record.source_value,
            display_name: record.display_name,
            created_at: record.created_at,
            last_opened_at: record.last_opened_at,
        }
    }
}

/// What a save produced: the persisted row, or a typed rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub(crate) enum SaveLiveViewDto {
    Saved {
        view: LiveViewDto,
        already_saved: bool,
    },
    Rejected {
        code: &'static str,
        reason: String,
    },
}

pub(crate) fn save_live_view_inner<M>(
    mediator: &M,
    data_root: &Path,
    path: String,
) -> Result<SaveLiveViewDto, String>
where
    M: Sender<SaveLiveView> + Handle,
{
    let response = mediator
        .send_now(SaveLiveView {
            data_root: data_root.to_path_buf(),
            path: path.into(),
        })
        .map_err(|err| format!("{err:#}"))?;
    Ok(match response.outcome {
        SaveLiveViewOutcome::Saved {
            record,
            already_saved,
        } => SaveLiveViewDto::Saved {
            view: record.into(),
            already_saved,
        },
        SaveLiveViewOutcome::Rejected { rejection } => SaveLiveViewDto::Rejected {
            code: rejection.code(),
            reason: rejection.to_string(),
        },
    })
}

#[tauri::command]
pub(crate) async fn save_live_view(
    mediator: tauri::State<'_, crate::WiredMediator>,
    path: String,
) -> Result<SaveLiveViewDto, String> {
    let mediator = mediator.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || save_live_view_inner(&mediator, &data_root, path))
        .await
        .map_err(|err| err.to_string())?
}

/// What probing a live-view source found, typed for the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub(crate) enum SourceProbeDto {
    Ok,
    Broken { code: &'static str, reason: String },
}

pub(crate) fn probe_source_inner<M>(
    mediator: &M,
    data_root: &Path,
    source_kind: String,
    source_value: String,
) -> Result<SourceProbeDto, String>
where
    M: Sender<ProbeSource> + Handle,
{
    let response = mediator
        .send_now(ProbeSource {
            data_root: data_root.to_path_buf(),
            source_kind,
            source_value,
        })
        .map_err(|err| format!("{err:#}"))?;
    Ok(match response.outcome {
        ProbeOutcome::Ok => SourceProbeDto::Ok,
        ProbeOutcome::Broken { rejection } => SourceProbeDto::Broken {
            code: rejection.code(),
            reason: rejection.to_string(),
        },
    })
}

#[tauri::command]
pub(crate) async fn probe_source(
    mediator: tauri::State<'_, crate::WiredMediator>,
    source_kind: String,
    source_value: String,
) -> Result<SourceProbeDto, String> {
    let mediator = mediator.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        probe_source_inner(&mediator, &data_root, source_kind, source_value)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn list_live_views(
    mediator: tauri::State<'_, crate::WiredMediator>,
) -> Result<Vec<LiveViewDto>, String> {
    let mediator = mediator.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        mediator
            .send_now(ListLiveViews { data_root })
            .map(|response| response.views.into_iter().map(LiveViewDto::from).collect())
            .map_err(|err| format!("{err:#}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn remove_live_view(
    mediator: tauri::State<'_, crate::WiredMediator>,
    source_kind: String,
    source_value: String,
) -> Result<bool, String> {
    let mediator = mediator.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        mediator
            .send_now(RemoveLiveView {
                data_root,
                source_kind,
                source_value,
            })
            .map(|response| response.removed)
            .map_err(|err| format!("{err:#}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn get_setting(
    mediator: tauri::State<'_, crate::WiredMediator>,
    key: String,
) -> Result<Option<String>, String> {
    let mediator = mediator.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        mediator
            .send_now(GetSetting { data_root, key })
            .map(|response| response.value)
            .map_err(|err| format!("{err:#}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn set_setting(
    mediator: tauri::State<'_, crate::WiredMediator>,
    key: String,
    value: String,
) -> Result<(), String> {
    let mediator = mediator.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        mediator
            .send_now(SetSetting {
                data_root,
                key,
                value,
            })
            .map(|_| ())
            .map_err(|err| format!("{err:#}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

#[cfg(test)]
mod tests {
    use application::{
        ports::RepoProbeResult,
        testing::{FakeDiffSource, FakeRepoProbe},
    };

    use super::*;

    #[test]
    fn rejected_save_carries_the_typed_code_and_reason() {
        // FakeRepoProbe::default() answers NotFound.
        let mediator = crate::test_support::fake_mediator(FakeDiffSource::default());

        let dto = save_live_view_inner(&mediator, Path::new("/data"), "/gone".into())
            .expect("save resolves");

        assert_eq!(
            dto,
            SaveLiveViewDto::Rejected {
                code: "DirNotFound",
                reason: "The git repo's directory at `/gone` was not found.".into(),
            }
        );
    }

    #[test]
    fn valid_save_maps_the_record() {
        let mediator = crate::test_support::fake_mediator_with_probe(
            FakeDiffSource::default(),
            FakeRepoProbe {
                result: RepoProbeResult::Repo {
                    top_level: "/repos/gt".into(),
                },
            },
        );

        let dto = save_live_view_inner(&mediator, Path::new("/data"), "/repos/gt".into())
            .expect("save resolves");

        let SaveLiveViewDto::Saved {
            view,
            already_saved,
        } = dto
        else {
            panic!("expected Saved");
        };
        assert!(!already_saved);
        assert_eq!(view.source_kind, "LocalRepo");
        assert_eq!(view.source_value, "/repos/gt");
        assert_eq!(view.display_name, "gt");
    }

    #[test]
    fn probe_reports_broken_for_a_missing_directory() {
        // FakeRepoProbe::default() answers NotFound.
        let mediator = crate::test_support::fake_mediator(FakeDiffSource::default());

        let dto = probe_source_inner(
            &mediator,
            Path::new("/data"),
            "LocalRepo".into(),
            "/gone".into(),
        )
        .expect("probe resolves");

        assert_eq!(
            dto,
            SourceProbeDto::Broken {
                code: "DirNotFound",
                reason: "The git repo's directory at `/gone` was not found.".into(),
            }
        );
    }

    #[test]
    fn probe_reports_ok_for_a_valid_repo() {
        let mediator = crate::test_support::fake_mediator_with_probe(
            FakeDiffSource::default(),
            FakeRepoProbe {
                result: RepoProbeResult::Repo {
                    top_level: "/repos/gt".into(),
                },
            },
        );

        let dto = probe_source_inner(
            &mediator,
            Path::new("/data"),
            "LocalRepo".into(),
            "/repos/gt".into(),
        )
        .expect("probe resolves");

        assert_eq!(dto, SourceProbeDto::Ok);
    }
}
