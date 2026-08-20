use gtl_application::projects::commit_repositories::{
    self, CommitAction, CommitExit, CommitRepositories, CommitRepositoriesError,
    CommitRepositoriesMode, CommitRepositoriesOk, CommitResult,
};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{project_client_error, run_blocking};
use crate::state::AppState;

pub(super) async fn execute(
    state: AppState,
    request: v1::CommitProjectRepositoriesRequest,
) -> Result<Response<v1::CommitProjectRepositoriesResponse>, Status> {
    let repos = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_client_error(&error))?;
    let mode = if request.dry_run {
        CommitRepositoriesMode::DryRun
    } else {
        CommitRepositoriesMode::Apply {
            message: request.message,
        }
    };
    let result = run_blocking(move || {
        commit_repositories::execute(CommitRepositories { repos, mode }, &state.git)
    })
    .await?;

    Ok(Response::new(commit_response(result)))
}

fn commit_response(
    result: Result<CommitRepositoriesOk, CommitRepositoriesError>,
) -> v1::CommitProjectRepositoriesResponse {
    match result {
        Ok(result) => v1::CommitProjectRepositoriesResponse {
            results: result.results.iter().map(commit_result).collect(),
            exit: match result.exit {
                CommitExit::Clean => v1::ProjectCommitExit::Clean,
                CommitExit::Warn => v1::ProjectCommitExit::Warning,
                CommitExit::Fail => v1::ProjectCommitExit::Failed,
            } as i32,
            failure_detail: None,
        },
        Err(error) => {
            let failure_detail = error.to_string();
            tracing::error!(error = ?error, "project commit stopped before completion");
            let completed_results = match error {
                CommitRepositoriesError::Transport {
                    mut completed_results,
                    failed_result,
                    ..
                } => {
                    if let Some(failed_result) = failed_result {
                        completed_results.push(*failed_result);
                    }
                    completed_results
                }
                _ => Vec::new(),
            };
            v1::CommitProjectRepositoriesResponse {
                results: completed_results.iter().map(commit_result).collect(),
                exit: v1::ProjectCommitExit::Failed as i32,
                failure_detail: Some(failure_detail),
            }
        }
    }
}

fn commit_result(result: &CommitResult) -> v1::ProjectCommitResult {
    v1::ProjectCommitResult {
        project_name: result.name().to_string(),
        present: result.is_present(),
        dirty: result.is_dirty(),
        files: result
            .files()
            .iter()
            .map(|file| v1::CommitFile {
                status: file.status.clone(),
                path: file.path.as_ref().to_string_lossy().into_owned(),
            })
            .collect(),
        action: match result.action() {
            CommitAction::Absent => v1::ProjectCommitAction::Absent,
            CommitAction::Clean => v1::ProjectCommitAction::Clean,
            CommitAction::WouldCommit => v1::ProjectCommitAction::WouldCommit,
            CommitAction::Skipped => v1::ProjectCommitAction::Skipped,
            CommitAction::Committed => v1::ProjectCommitAction::Committed,
            CommitAction::Fail => v1::ProjectCommitAction::Failed,
        } as i32,
        detail: result.detail().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::projects::commit_repositories::{CommitOutcome, CommitResult};
    use gtl_models::paths::ProjectName;

    use super::*;

    #[test]
    fn commit_projection_preserves_closed_result_facts() {
        let result = CommitResult::new(ProjectName::try_new("api").unwrap(), CommitOutcome::Clean);
        let projected = commit_result(&result);

        assert_eq!(projected.project_name, "api");
        assert!(projected.present);
        assert!(!projected.dirty);
        assert_eq!(projected.action(), v1::ProjectCommitAction::Clean);
        assert_eq!(projected.detail, "nothing to commit");
    }
}
