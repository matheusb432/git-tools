use gtl_models::paths::AbsoluteFilePath;
use gtl_wire::v1;

use crate::cli::DiffTarget;

pub enum DiffOutcome {
    Rendered(AbsoluteFilePath),
    Empty,
    Forwarded,
}

pub fn run(target: &DiffTarget, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let working_directory = super::canonical_working_directory()?
        .to_string_lossy()
        .into_owned();
    let prepare = v1::PrepareDiffRequest {
        working_directory: working_directory.clone(),
        target: Some(grpc_target(target)),
        name: name.map(str::to_string),
    };
    let render = v1::RenderDiffRequest {
        working_directory,
        target: Some(grpc_target(target)),
        name: name.map(str::to_string),
    };
    super::present(
        raw,
        |client| client.prepare_diff(prepare),
        |client| client.render_diff(render),
    )
}

pub(crate) fn grpc_target(target: &DiffTarget) -> v1::DiffTarget {
    let selection = match target {
        DiffTarget::Unpushed { .. } => v1::diff_target::Selection::Unpushed(v1::Empty {}),
        DiffTarget::Base(revision) => {
            v1::diff_target::Selection::BaseRevision(revision.to_string())
        }
        DiffTarget::Range { range, .. } => {
            v1::diff_target::Selection::RevisionRange(range.to_string())
        }
        DiffTarget::Merge { base, .. } => v1::diff_target::Selection::MergeBase(base.to_string()),
        DiffTarget::Last { count, .. } => v1::diff_target::Selection::LastCommitCount(count.get()),
    };
    v1::DiffTarget {
        selection: Some(selection),
    }
}
