//! Wire DTO <-> application-type mapping for project repository endpoints.

pub mod pull_all;
pub mod push_all;

use gtl_application::projects::{
    RepoSyncResult, SyncExit, SyncStatus,
    pull_repositories::{PullRepositories, PullRepositoriesOk},
    push_repositories::{PushRepositories, PushRepositoriesOk},
};
use gtl_models::git::GitEffectMode;
use gtl_wire::{
    envelope::{Envelope, Outcome},
    projects::{
        PullAllRequest, PushAllRequest, RepoSyncResultDto, RepoSyncStatusDto, SyncData, SyncExitDto,
    },
};

pub(crate) fn to_push_all_request(dto: &PushAllRequest) -> PushRepositories {
    PushRepositories {
        mode: effect_mode(dto.dry),
    }
}

pub(crate) fn to_pull_all_request(dto: &PullAllRequest) -> PullRepositories {
    PullRepositories {
        mode: effect_mode(dto.dry),
    }
}

const fn effect_mode(dry: bool) -> GitEffectMode {
    if dry {
        GitEffectMode::DryRun
    } else {
        GitEffectMode::Apply
    }
}

fn to_exit_dto(exit: SyncExit) -> SyncExitDto {
    match exit {
        SyncExit::Clean => SyncExitDto::Clean,
        SyncExit::Warn => SyncExitDto::Warn,
        SyncExit::Fail => SyncExitDto::Fail,
    }
}

fn to_status_dto(status: SyncStatus) -> RepoSyncStatusDto {
    match status {
        SyncStatus::Skip => RepoSyncStatusDto::Skip,
        SyncStatus::UpToDate => RepoSyncStatusDto::UpToDate,
        SyncStatus::Pushed => RepoSyncStatusDto::Pushed,
        SyncStatus::WouldPush => RepoSyncStatusDto::WouldPush,
        SyncStatus::Pulled => RepoSyncStatusDto::Pulled,
        SyncStatus::WouldPull => RepoSyncStatusDto::WouldPull,
        SyncStatus::Warn => RepoSyncStatusDto::Warn,
        SyncStatus::Fail => RepoSyncStatusDto::Fail,
    }
}

fn to_result_dto(result: RepoSyncResult) -> RepoSyncResultDto {
    RepoSyncResultDto {
        name: result.name,
        branch: result.branch,
        status: to_status_dto(result.status),
        detail: result.detail,
    }
}

fn ok_envelope(results: Vec<RepoSyncResult>, exit: SyncExit) -> Envelope<SyncData> {
    Envelope {
        outcome: Outcome::Ok,
        notes: Vec::new(),
        data: Some(SyncData {
            results: results.into_iter().map(to_result_dto).collect(),
            exit: to_exit_dto(exit),
        }),
    }
}

pub(crate) fn to_push_all_envelope(resp: PushRepositoriesOk) -> Envelope<SyncData> {
    ok_envelope(resp.results, resp.exit)
}

pub(crate) fn to_pull_all_envelope(resp: PullRepositoriesOk) -> Envelope<SyncData> {
    ok_envelope(resp.results, resp.exit)
}
