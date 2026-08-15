//! `/tags/*` endpoints and their wire-to-application mappings.

pub mod bump;
pub mod dry_run;

use std::path::PathBuf;

use gtl_application::tags::{
    BumpLevel, TagActionStatus,
    bump_tag::{BumpTag, BumpTagOk},
    dry_run_tag_bump::{DryRunTagBump, DryRunTagBumpOk, TagBumpPreview},
};
use gtl_wire::{
    envelope::{Envelope, Note, NoteLevel, Outcome},
    tags::{BumpTagData, BumpTagRequest, DryRunTagBumpRequest, TagBumpLevelDto, TagBumpStatusDto},
};

pub(crate) fn to_dry_run_request(
    dto: DryRunTagBumpRequest,
) -> Result<DryRunTagBump, crate::endpoints::EndpointError> {
    let repo_path = absolute_path(dto.repo_path)?;
    Ok(DryRunTagBump {
        repo_path,
        level: from_level_dto(dto.level),
        message: dto.message,
        push: dto.push,
    })
}

pub(crate) fn to_dry_run_envelope(
    response: DryRunTagBumpOk,
) -> Envelope<gtl_wire::tags::TagBumpPreview> {
    match response {
        DryRunTagBumpOk::Ready(preview) => Envelope {
            outcome: Outcome::Ok,
            notes: Vec::new(),
            data: Some(to_preview_dto(preview)),
        },
        DryRunTagBumpOk::Rejected { detail } => error_envelope(detail, None),
    }
}

pub(crate) fn to_bump_request(
    dto: BumpTagRequest,
) -> Result<BumpTag, crate::endpoints::EndpointError> {
    let preview = dto.preview;
    let repo_path = absolute_path(preview.repo_path)?;
    Ok(BumpTag {
        preview: TagBumpPreview {
            repo_path,
            branch: preview.branch,
            target_id: preview.target_id,
            level: from_level_dto(preview.level),
            base_tag: preview.base_tag,
            next_tag: preview.next_tag,
            message: preview.message,
            push: preview.push,
        },
    })
}

pub(crate) fn to_bump_envelope(response: BumpTagOk) -> Envelope<BumpTagData> {
    match response {
        BumpTagOk::Applied { tag, outcome } => {
            let status = to_status_dto(outcome.status);
            let data = BumpTagData {
                tag,
                status,
                detail: outcome.detail.clone(),
            };
            if outcome.status == TagActionStatus::Failed {
                error_envelope(outcome.detail, Some(data))
            } else {
                Envelope {
                    outcome: Outcome::Ok,
                    notes: (!outcome.detail.is_empty())
                        .then_some(Note {
                            level: NoteLevel::Info,
                            text: outcome.detail,
                        })
                        .into_iter()
                        .collect(),
                    data: Some(data),
                }
            }
        }
        BumpTagOk::Rejected { detail } => error_envelope(detail, None),
    }
}

fn absolute_path(value: String) -> Result<PathBuf, crate::endpoints::EndpointError> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(crate::endpoints::EndpointError::bad_request(
            "repo_path must be absolute",
        ))
    }
}

fn to_preview_dto(preview: TagBumpPreview) -> gtl_wire::tags::TagBumpPreview {
    gtl_wire::tags::TagBumpPreview {
        repo_path: preview.repo_path.to_string_lossy().into_owned(),
        branch: preview.branch,
        target_id: preview.target_id,
        level: to_level_dto(preview.level),
        base_tag: preview.base_tag,
        next_tag: preview.next_tag,
        message: preview.message,
        push: preview.push,
    }
}

fn from_level_dto(level: TagBumpLevelDto) -> BumpLevel {
    match level {
        TagBumpLevelDto::Major => BumpLevel::Major,
        TagBumpLevelDto::Minor => BumpLevel::Minor,
        TagBumpLevelDto::Patch => BumpLevel::Patch,
    }
}

fn to_level_dto(level: BumpLevel) -> TagBumpLevelDto {
    match level {
        BumpLevel::Major => TagBumpLevelDto::Major,
        BumpLevel::Minor => TagBumpLevelDto::Minor,
        BumpLevel::Patch => TagBumpLevelDto::Patch,
    }
}

fn to_status_dto(status: TagActionStatus) -> TagBumpStatusDto {
    match status {
        TagActionStatus::Created => TagBumpStatusDto::Created,
        TagActionStatus::Noop => TagBumpStatusDto::Noop,
        TagActionStatus::Pushed => TagBumpStatusDto::Pushed,
        TagActionStatus::Failed => TagBumpStatusDto::Failed,
    }
}

fn error_envelope<D>(detail: String, data: Option<D>) -> Envelope<D> {
    Envelope {
        outcome: Outcome::Error,
        notes: vec![Note {
            level: NoteLevel::Error,
            text: detail,
        }],
        data,
    }
}
