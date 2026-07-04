//! Wire DTO <-> application-type mapping for the managed feature's daemon endpoints.

pub mod pull_all;
pub mod push_all;

use application::managed::{
    pull_all::{PullAll, PullAllResponse},
    push_all::{PushAll, PushAllResponse},
    service::{RepoSyncResult, SyncExit},
};
use contracts::{
    envelope::{Envelope, Note, NoteLevel, Outcome},
    managed::{PullAllRequest, PushAllRequest, RepoSyncResultDto, SyncData, SyncExitDto},
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

fn to_result_dto(result: RepoSyncResult) -> RepoSyncResultDto {
    RepoSyncResultDto {
        name: result.name,
        branch: result.branch,
        status: result.status,
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

pub(crate) fn to_push_all_envelope(resp: PushAllResponse) -> Envelope<SyncData> {
    ok_envelope(resp.results, resp.exit)
}

pub(crate) fn to_pull_all_envelope(resp: PullAllResponse) -> Envelope<SyncData> {
    ok_envelope(resp.results, resp.exit)
}

pub(crate) fn error_envelope(text: String) -> Envelope<SyncData> {
    Envelope {
        outcome: Outcome::Error,
        notes: vec![Note {
            level: NoteLevel::Error,
            text,
        }],
        data: None,
    }
}
