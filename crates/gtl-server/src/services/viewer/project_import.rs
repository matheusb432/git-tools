use std::path::{Path, PathBuf};

use gtl_application::{
    projects::{
        discover_project_repositories,
        import_project_repositories::{self, ImportProjectRepositoryOk},
    },
    repositories::find_repositories::{self, FindRepositories},
};
use gtl_models::{
    failure::{ExternalDiagnostic, ProjectFailure, ScanFolderProblem},
    repository::traversal::RepositoryTraversalScope,
};
use gtl_wire::{
    proto, v1,
    viewer::projects::{ProjectImportOutcome, ProjectImportResult},
};
use tonic::{Request, Response, Status};

use super::super::{
    run_blocking,
    status::{failure, invalid_request, status},
    unexpected,
};
use crate::state::AppState;

pub(super) async fn discover(
    state: &AppState,
    request: Request<v1::DiscoverProjectRepositoriesRequest>,
) -> Result<Response<v1::DiscoverProjectRepositoriesResponse>, Status> {
    let request = proto::viewer::projects::decode_discover(request.into_inner())
        .map_err(|_| invalid_request("root"))?;
    let root = resolve_scan_root(&request.root)?;
    let state = state.clone();
    let discovery = run_blocking(move || {
        let repositories = find_repositories::execute(FindRepositories {
            root: root.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .map_err(|error| scan_failed(&error))?;
        if repositories.len() > usize::from(gtl_models::projects::catalogue::PROJECTS_MAX) {
            return Err(status(&ProjectFailure::TooManyRepositories {
                repositories_max: u32::from(gtl_models::projects::catalogue::PROJECTS_MAX),
            }));
        }
        let connection = state
            .database
            .connection_lock()
            .map_err(|error| unexpected(error, "open project catalogue for discovery"))?;
        discover_project_repositories::execute(
            discover_project_repositories::AnnotateProjectRepositories {
                root: root.to_string_lossy().into_owned(),
                repositories,
            },
            &connection,
        )
        .map_err(|error| unexpected(error, "annotate discovered repositories"))
    })
    .await??;
    Ok(Response::new(proto::viewer::projects::encode_discovery(
        discovery,
    )))
}

fn resolve_scan_root(input: &str) -> Result<PathBuf, Status> {
    let home = || {
        directories::BaseDirs::new()
            .map(|directories| directories.home_dir().to_path_buf())
            .ok_or_else(|| status(&ProjectFailure::HomeUnavailable))
    };
    let path = if input == "~" {
        home()?
    } else if let Some(relative) = input.strip_prefix("~/") {
        home()?.join(relative)
    } else {
        let path = Path::new(input);
        if !path.is_absolute() {
            return Err(scan_folder_invalid(ScanFolderProblem::NotAbsolute));
        }
        path.to_path_buf()
    };
    let canonical = path.canonicalize().map_err(|error| scan_failed(&error))?;
    if !canonical.is_dir() {
        return Err(scan_folder_invalid(ScanFolderProblem::NotDirectory));
    }
    if canonical.to_str().is_none() {
        return Err(scan_folder_invalid(ScanFolderProblem::NotUtf8));
    }
    Ok(canonical)
}

fn scan_failed(error: &impl std::fmt::Display) -> Status {
    status(&ProjectFailure::ScanFailed {
        diagnostic: ExternalDiagnostic::new(&error.to_string()),
    })
}

fn scan_folder_invalid(problem: ScanFolderProblem) -> Status {
    status(&ProjectFailure::ScanFolderInvalid { problem })
}

pub(super) async fn import(
    state: &AppState,
    request: Request<v1::ImportProjectRepositoriesRequest>,
) -> Result<Response<v1::ImportProjectRepositoriesResponse>, Status> {
    let request = proto::viewer::projects::decode_import(request.into_inner())
        .map_err(|_| invalid_request("repositories"))?;
    let state = state.clone();
    let attempts = run_blocking(move || {
        let mut connection = state
            .database
            .connection_lock()
            .map_err(|error| unexpected(error, "open project catalogue for import"))?;
        Ok::<_, Status>(import_project_repositories::execute(
            request,
            &mut connection,
        ))
    })
    .await??;
    let results = attempts
        .into_iter()
        .map(|attempt| ProjectImportResult {
            outcome: match &attempt.result {
                Ok(ImportProjectRepositoryOk::Created) => ProjectImportOutcome::Created,
                Ok(ImportProjectRepositoryOk::Restored) => ProjectImportOutcome::Restored,
                Err(error) => ProjectImportOutcome::Failed(failure(error)),
            },
            path: attempt.path,
            project_id: attempt.project_id,
        })
        .collect();
    Ok(Response::new(
        proto::viewer::projects::encode_import_results(results),
    ))
}

#[cfg(test)]
mod tests {
    use super::resolve_scan_root;

    #[test]
    fn home_relative_root_is_displayed_as_a_canonical_absolute_path() {
        let home = directories::BaseDirs::new().unwrap();
        let directory = tempfile::Builder::new()
            .prefix(".gtl-scan-root-")
            .tempdir_in(home.home_dir())
            .unwrap();
        let name = directory.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(
            resolve_scan_root(&format!("~/{name}")).unwrap(),
            directory.path().canonicalize().unwrap()
        );
    }
}
