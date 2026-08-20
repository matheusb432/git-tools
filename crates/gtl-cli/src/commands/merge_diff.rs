use std::path::Path;

use gtl_models::git::GitRevision;
use gtl_wire::v1;

use crate::commands::diff::DiffOutcome;

pub fn run(
    repo_path: impl AsRef<Path>,
    base: Option<&str>,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let working_directory = std::path::absolute(repo_path.as_ref())?
        .to_string_lossy()
        .into_owned();
    let base_revision = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(|base| GitRevision::try_new(base.to_owned()))
        .transpose()
        .map_err(|_| anyhow::anyhow!("merge base must not be empty"))?
        .map(|base| base.to_string());
    let prepare = v1::PrepareMergeDiffRequest {
        working_directory: working_directory.clone(),
        base_revision: base_revision.clone(),
    };
    let render = v1::RenderMergeDiffRequest {
        working_directory,
        base_revision,
    };
    super::present(
        raw,
        |client| client.prepare_merge_diff(prepare),
        |client| client.render_merge_diff(render),
    )
}
