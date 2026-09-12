use std::path::Path;

use anyhow::Context as _;
pub use gtl_models::repository::status::StatusResult;
use gtl_models::{
    git::{BranchName, CommitCount, GitRefName},
    paths::ProjectName,
    repository::{
        PathCount,
        status::{StatusChanges, StatusHead, StatusUpstream},
    },
};
use gtl_wire::v1;

use self::palette::StatusColorPalette;
use super::{ManagedExit, ManagedOptions, ManagedRun};
use crate::server_client::ServerClient;
mod palette;

#[must_use]
pub fn run_status(options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let response =
        ServerClient::connect().and_then(|client| client.get_project_repository_statuses());
    status_response(response, options)
}

/// Returns exit 2 outside a repository.
#[must_use]
pub fn run_status_current(dir: &Path, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let response = ServerClient::connect().and_then(|client| {
        client.get_repository_status(v1::GetRepositoryStatusRequest {
            repository_path: dir.to_string_lossy().into_owned(),
        })
    });
    status_response(response, options)
}

/// Returns exit 2 when no repositories are found.
#[must_use]
pub fn run_status_recursive(root: &Path, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    let response = ServerClient::connect().and_then(|client| {
        client.get_recursive_repository_statuses(v1::GetRecursiveRepositoryStatusesRequest {
            root: root.to_string_lossy().into_owned(),
        })
    });
    status_response(response, options)
}

fn status_response<Response>(
    response: anyhow::Result<Response>,
    options: &ManagedOptions,
) -> ManagedRun<StatusResult>
where
    Response: Into<RepositoryStatuses>,
{
    match response.and_then(|response| {
        let response: RepositoryStatuses = response.into();
        response
            .results
            .into_iter()
            .map(status_result_from_grpc)
            .collect()
    }) {
        Ok(results) => status_run(results, options),
        Err(error) => status_fail(format!("status: {}", crate::error_text(&error))),
    }
}

struct RepositoryStatuses {
    results: Vec<v1::RepositoryStatusResult>,
}

impl From<v1::GetProjectRepositoryStatusesResponse> for RepositoryStatuses {
    fn from(response: v1::GetProjectRepositoryStatusesResponse) -> Self {
        Self {
            results: response.results,
        }
    }
}

impl From<v1::GetRepositoryStatusResponse> for RepositoryStatuses {
    fn from(response: v1::GetRepositoryStatusResponse) -> Self {
        Self {
            results: response.results,
        }
    }
}

impl From<v1::GetRecursiveRepositoryStatusesResponse> for RepositoryStatuses {
    fn from(response: v1::GetRecursiveRepositoryStatusesResponse) -> Self {
        Self {
            results: response.results,
        }
    }
}

fn status_result_from_grpc(result: v1::RepositoryStatusResult) -> anyhow::Result<StatusResult> {
    let name = ProjectName::try_new(result.project_name)
        .context("gtl-server returned an empty repository status name")?;
    match result
        .state
        .context("gtl-server omitted repository status state")?
    {
        v1::repository_status_result::State::Absent(_) => Ok(StatusResult::absent(name)),
        v1::repository_status_result::State::Present(present) => Ok(StatusResult::present(
            name,
            status_head_from_grpc(
                present
                    .head
                    .context("gtl-server omitted repository status head")?,
            )?,
            status_changes_from_grpc(
                present
                    .changes
                    .context("gtl-server omitted repository status changes")?,
            )?,
        )),
    }
}

fn status_head_from_grpc(head: v1::RepositoryStatusHead) -> anyhow::Result<StatusHead> {
    match head
        .state
        .context("gtl-server omitted repository head state")?
    {
        v1::repository_status_head::State::Unavailable(_) => Ok(StatusHead::Unavailable),
        v1::repository_status_head::State::Detached(_) => Ok(StatusHead::Detached),
        v1::repository_status_head::State::Branch(branch) => Ok(StatusHead::Branch {
            name: BranchName::try_new(branch.name)
                .context("gtl-server returned an empty repository branch")?,
            upstream: match branch
                .upstream
                .context("gtl-server omitted repository upstream state")?
            {
                v1::repository_status_branch::Upstream::Missing(_) => StatusUpstream::Missing,
                v1::repository_status_branch::Upstream::Tracking(tracking) => {
                    StatusUpstream::Tracking {
                        reference: GitRefName::try_new(tracking.reference)
                            .context("gtl-server returned an empty upstream reference")?,
                        ahead: CommitCount::new(tracking.commits_ahead),
                    }
                }
            },
        }),
    }
}

fn status_changes_from_grpc(changes: v1::RepositoryStatusChanges) -> anyhow::Result<StatusChanges> {
    match changes
        .state
        .context("gtl-server omitted repository changes state")?
    {
        v1::repository_status_changes::State::Clean(_) => Ok(StatusChanges::Clean),
        v1::repository_status_changes::State::Changed(counts) => Ok(StatusChanges::from_counts(
            PathCount::new(counts.tracked_paths),
            PathCount::new(counts.untracked_paths),
        )),
        v1::repository_status_changes::State::Unavailable(_) => Ok(StatusChanges::Unavailable),
    }
}

fn status_run(results: Vec<StatusResult>, options: &ManagedOptions) -> ManagedRun<StatusResult> {
    match format_status(
        options.output.is_json(),
        options.output.color_enabled(),
        &results,
    ) {
        Ok(stdout) => ManagedRun {
            exit: ManagedExit::Clean,
            results,
            stdout,
            stderr: String::new(),
        },
        Err(error) => status_fail(format!("status: {}", crate::error_text(&error))),
    }
}

fn status_fail(message: String) -> ManagedRun<StatusResult> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: message,
    }
}

fn format_status(json: bool, color: bool, results: &[StatusResult]) -> anyhow::Result<String> {
    if json {
        return serde_json::to_string_pretty(results).map_err(Into::into);
    }

    let palette = color
        .then(StatusColorPalette::from_embedded_toml)
        .transpose()?;
    Ok(results
        .iter()
        .map(|result| format_status_line(result, palette.as_ref()))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn format_status_line(result: &StatusResult, palette: Option<&StatusColorPalette>) -> String {
    let detail = result.detail();
    if !result.is_present() {
        return format!(
            "{} {}",
            result.name(),
            format_bracketed_status(&detail, palette)
        );
    }

    let branch = result.branch_label().unwrap_or("(unknown)");
    format!(
        "{} {} {}",
        result.name(),
        branch,
        format_bracketed_status(&detail, palette)
    )
}

fn format_bracketed_status(detail: &str, palette: Option<&StatusColorPalette>) -> String {
    let Some(palette) = palette else {
        return format!("[{detail}]");
    };

    format!(
        "\x1b[1m{}{}{}\x1b[0m",
        palette.brackets.paint("["),
        colorize_status_detail(detail, palette),
        palette.brackets.paint("]")
    )
}

fn colorize_status_detail(detail: &str, palette: &StatusColorPalette) -> String {
    detail
        .split_whitespace()
        .map(|token| match token {
            "✓" => palette.checkmark.paint(token),
            "!" | "?" | "!?" | "?!" => palette.change_markers.paint(token),
            ahead if ahead.starts_with('⇡') => colorize_ahead(ahead, palette),
            _ => token.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn colorize_ahead(value: &str, palette: &StatusColorPalette) -> String {
    let Some(count) = value.strip_prefix('⇡') else {
        return value.to_string();
    };
    format!("{}{}", palette.ahead_arrow.paint("⇡"), count)
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::{BranchName, CommitCount, GitRefName},
        repository::{
            PathCount,
            status::{StatusChanges, StatusHead, StatusUpstream},
        },
    };

    use super::*;
    use crate::testing::project_name;

    fn branch_status(name: &str, ahead: u64, changes: StatusChanges) -> StatusResult {
        StatusResult::present(
            project_name(name),
            StatusHead::Branch {
                name: BranchName::main(),
                upstream: StatusUpstream::Tracking {
                    reference: GitRefName::try_new("origin/main").unwrap(),
                    ahead: CommitCount::new(ahead),
                },
            },
            changes,
        )
    }

    #[test]
    fn status_formatting_preserves_present_and_absent_rows() {
        let results = [
            branch_status(
                "repo",
                1,
                StatusChanges::from_counts(PathCount::new(1), PathCount::new(1)),
            ),
            StatusResult::absent(project_name("missing")),
        ];

        assert_eq!(
            format_status(false, false, &results).unwrap(),
            "repo main [⇡1 !?]\nmissing [not present]"
        );
    }

    #[test]
    fn status_formatting_colors_brackets_ahead_changes_and_checkmarks() {
        let results = [
            branch_status(
                "dirty",
                1,
                StatusChanges::from_counts(PathCount::new(1), PathCount::new(1)),
            ),
            branch_status("clean", 0, StatusChanges::Clean),
        ];
        let rendered = format_status(false, true, &results).unwrap();

        assert!(rendered.contains("\x1b[1m\x1b[38;2;242;133;0m[\x1b[39m"));
        assert!(rendered.contains("\x1b[38;2;242;133;0m⇡\x1b[39m1"));
        assert!(rendered.contains("\x1b[38;2;255;77;77m!?\x1b[39m"));
        assert!(rendered.contains("\x1b[38;2;46;204;113m✓\x1b[39m"));
    }
}
