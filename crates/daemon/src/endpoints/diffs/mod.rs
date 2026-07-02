//! The `diffs` endpoints and the one place mapping wire DTOs ↔ domain/application
//! types (template pattern: `endpoints/todos/mod.rs`).

pub mod all;
pub mod merge;
pub mod render;
pub mod squash_preview;
pub mod subrepos;

use application::{
    diffs::{
        batch::RepoRef,
        render_diff::{RenderDiff, RenderDiffOutcome, RenderDiffResponse},
        render_diff_all::{RenderDiffAll, RenderDiffAllResponse},
        render_diff_subrepos::{
            RenderDiffSubrepos, RenderDiffSubreposOutcome, RenderDiffSubreposResponse,
        },
        render_merge_diff::{RenderMergeDiff, RenderMergeDiffResponse},
        render_squash_preview::{RenderSquashPreview, RenderSquashPreviewResponse},
    },
    shared::notes as app_notes,
};
use contracts::{
    diffs::{
        DiffTargetDto, RenderDiffAllRequest, RenderDiffData, RenderDiffRequest,
        RenderDiffSubreposRequest, RenderMergeDiffRequest, RenderSquashPreviewRequest, RepoRefDto,
    },
    envelope::{Envelope, Note, NoteLevel, Outcome},
};
use domain::diffs::DiffTarget;

/// Map the wire request DTO onto the application request.
///
/// # Errors
/// Returns an error when the DTO carries an invalid selection (e.g. a `last` count of zero).
pub(crate) fn to_request(dto: RenderDiffRequest) -> anyhow::Result<RenderDiff> {
    let target = to_target(dto.target)?;
    Ok(RenderDiff {
        cwd: dto.cwd.into(),
        store_root: dto.store_root.into(),
        target,
        name: dto.name,
        theme: dto.theme,
    })
}

/// Map the wire target DTO onto the domain target, shared by [`to_request`] and
/// [`to_subrepos_request`].
///
/// # Errors
/// Returns an error when the DTO carries an invalid selection (e.g. a `last` count of zero).
fn to_target(dto: DiffTargetDto) -> anyhow::Result<DiffTarget> {
    Ok(match dto {
        DiffTargetDto::Unpushed => DiffTarget::Unpushed,
        DiffTargetDto::Base { rev } => DiffTarget::Base(rev),
        DiffTargetDto::Range { range } => DiffTarget::Range(range),
        DiffTargetDto::Merge { base } => DiffTarget::Merge(base),
        DiffTargetDto::Last { count } => DiffTarget::Last(
            std::num::NonZeroU32::new(count)
                .ok_or_else(|| anyhow::anyhow!("last count must be >= 1"))?,
        ),
    })
}

/// Map a wire repo reference onto the application's.
fn to_repo_ref(dto: RepoRefDto) -> RepoRef {
    RepoRef {
        top: dto.top,
        label: dto.label,
    }
}

/// Map the wire request DTO onto the application request.
pub(crate) fn to_merge_request(dto: RenderMergeDiffRequest) -> RenderMergeDiff {
    RenderMergeDiff {
        cwd: dto.cwd.into(),
        store_root: dto.store_root.into(),
        base: dto.base,
    }
}

/// Map the wire request DTO onto the application request.
pub(crate) fn to_squash_request(dto: RenderSquashPreviewRequest) -> RenderSquashPreview {
    RenderSquashPreview {
        cwd: dto.cwd.into(),
        store_root: dto.store_root.into(),
    }
}

/// Map the wire request DTO onto the application request.
///
/// # Errors
/// Returns an error when the DTO carries an invalid target selection (e.g. a `last` count of
/// zero).
pub(crate) fn to_subrepos_request(
    dto: RenderDiffSubreposRequest,
) -> anyhow::Result<RenderDiffSubrepos> {
    Ok(RenderDiffSubrepos {
        store_root: dto.store_root.into(),
        root: dto.root.into(),
        target: to_target(dto.target)?,
        repos: dto.repos.into_iter().map(to_repo_ref).collect(),
        theme: dto.theme,
    })
}

/// Map the wire request DTO onto the application request.
pub(crate) fn to_all_request(dto: RenderDiffAllRequest) -> RenderDiffAll {
    RenderDiffAll {
        store_root: dto.store_root.into(),
        root: dto.root.into(),
        repos: dto.repos.into_iter().map(to_repo_ref).collect(),
        theme: dto.theme,
    }
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_envelope(resp: RenderDiffResponse) -> Envelope<RenderDiffData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        RenderDiffOutcome::Rendered { artifact, reused } => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(RenderDiffData {
                artifact: artifact.to_string_lossy().into_owned(),
                reused,
            }),
        },
        RenderDiffOutcome::Empty => Envelope {
            outcome: Outcome::Empty,
            notes,
            data: None,
        },
    }
}

/// Build an error envelope carrying a single `Error`-level note.
pub(crate) fn error_envelope(text: String) -> Envelope<RenderDiffData> {
    Envelope {
        outcome: Outcome::Error,
        notes: vec![Note {
            level: NoteLevel::Error,
            text,
        }],
        data: None,
    }
}

/// Shared by every response shaped `{artifact, reused, notes}`.
fn ok_envelope(
    artifact: std::path::PathBuf,
    reused: bool,
    notes: &[app_notes::Note],
) -> Envelope<RenderDiffData> {
    Envelope {
        outcome: Outcome::Ok,
        notes: notes.iter().map(to_note).collect(),
        data: Some(RenderDiffData {
            artifact: artifact.to_string_lossy().into_owned(),
            reused,
        }),
    }
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_merge_envelope(resp: RenderMergeDiffResponse) -> Envelope<RenderDiffData> {
    ok_envelope(resp.artifact, resp.reused, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_squash_envelope(resp: RenderSquashPreviewResponse) -> Envelope<RenderDiffData> {
    ok_envelope(resp.artifact, resp.reused, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_all_envelope(resp: RenderDiffAllResponse) -> Envelope<RenderDiffData> {
    ok_envelope(resp.artifact, resp.reused, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_subrepos_envelope(resp: RenderDiffSubreposResponse) -> Envelope<RenderDiffData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        RenderDiffSubreposOutcome::Rendered { artifact, reused } => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(RenderDiffData {
                artifact: artifact.to_string_lossy().into_owned(),
                reused,
            }),
        },
        RenderDiffSubreposOutcome::Empty => Envelope {
            outcome: Outcome::Empty,
            notes,
            data: None,
        },
    }
}

/// Map an application note (Info/Warn only) onto its wire counterpart.
fn to_note(n: &app_notes::Note) -> Note {
    Note {
        level: match n.level {
            app_notes::NoteLevel::Info => NoteLevel::Info,
            app_notes::NoteLevel::Warn => NoteLevel::Warn,
        },
        text: n.text.clone(),
    }
}
