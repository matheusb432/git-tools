//! Wire DTO <-> application-type mapping for the managed feature's daemon endpoints.

pub mod pull_all;
pub mod push_all;

use application::managed::{
    pull_all::{PullAll, PullAllOk},
    push_all::{PushAll, PushAllOk},
    service::{RepoSyncResult, SyncExit, SyncStatus},
};
use contracts::{
    envelope::{Envelope, Outcome},
    managed::{
        PullAllRequest, PushAllRequest, RepoSyncResultDto, RepoSyncStatusDto, SyncData, SyncExitDto,
    },
};

pub(crate) fn to_push_all_request(dto: PushAllRequest) -> PushAll {
    PushAll {
        repos_file: dto.repos_file.into(),
        home_dir: dto.home_dir.into(),
        dry: dto.dry,
    }
}

pub(crate) fn to_pull_all_request(dto: PullAllRequest) -> PullAll {
    PullAll {
        repos_file: dto.repos_file.into(),
        home_dir: dto.home_dir.into(),
        dry: dto.dry,
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

pub(crate) fn to_push_all_envelope(resp: PushAllOk) -> Envelope<SyncData> {
    ok_envelope(resp.results, resp.exit)
}

pub(crate) fn to_pull_all_envelope(resp: PullAllOk) -> Envelope<SyncData> {
    ok_envelope(resp.results, resp.exit)
}
