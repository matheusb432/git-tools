use std::path::{Path, PathBuf};

use gtl_application::{
    projects::{discover_project_repositories, import_project_repositories},
    repositories::find_repositories::{self, FindRepositories},
};
use gtl_models::repository::traversal::RepositoryTraversalScope;
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::{run_blocking, unexpected};
use crate::state::AppState;

pub(super) async fn discover(
    state: &AppState,
    request: Request<v1::DiscoverProjectRepositoriesRequest>,
) -> Result<Response<v1::DiscoverProjectRepositoriesResponse>, Status> {
    let request = proto::viewer::projects::decode_discover(request.into_inner())
        .map_err(|_| Status::invalid_argument("scan folder is required"))?;
    let root = resolve_scan_root(&request.root)?;
    let state = state.clone();
    let discovery = run_blocking(move || {
        let repositories = find_repositories::execute(FindRepositories {
            root: root.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .map_err(|error| Status::failed_precondition(format!("could not scan folder: {error}")))?;
        if repositories.len() > usize::from(gtl_models::projects::catalogue::PROJECTS_MAX) {
            return Err(Status::resource_exhausted(
                "too many repositories found in this folder",
            ));
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
    let path = if input == "~" {
        directories::BaseDirs::new()
            .ok_or_else(|| Status::failed_precondition("home directory is unavailable"))?
            .home_dir()
            .to_path_buf()
    } else if let Some(relative) = input.strip_prefix("~/") {
        directories::BaseDirs::new()
            .ok_or_else(|| Status::failed_precondition("home directory is unavailable"))?
            .home_dir()
            .join(relative)
    } else {
        let path = Path::new(input);
        if !path.is_absolute() {
            return Err(Status::invalid_argument(
                "scan folder must be absolute or start with ~/",
            ));
        }
        path.to_path_buf()
    };
    let canonical = path.canonicalize().map_err(|error| {
        Status::failed_precondition(format!("could not open scan folder: {error}"))
    })?;
    if !canonical.is_dir() {
        return Err(Status::invalid_argument("scan folder is not a directory"));
    }
    if canonical.to_str().is_none() {
        return Err(Status::invalid_argument("scan folder is not UTF-8"));
    }
    Ok(canonical)
}

pub(super) async fn import(
    state: &AppState,
    request: Request<v1::ImportProjectRepositoriesRequest>,
) -> Result<Response<v1::ImportProjectRepositoriesResponse>, Status> {
    let request = proto::viewer::projects::decode_import(request.into_inner())
        .map_err(|_| Status::invalid_argument("select at least one repository to import"))?;
    let state = state.clone();
    let results = run_blocking(move || {
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
