//! Git repository operations, including branch transitions, local synchronization,
//! recursive traversal, status inspection, and recursive pushes.

pub mod apply_push;
pub mod apply_recursive_push;
pub mod build_recipes;
pub mod find_repositories;
pub mod find_repository_roots;
pub mod get_recursive_repository_statuses;
pub mod get_repository_status;
pub mod get_repository_statuses;
pub mod plan_push;
pub mod plan_recursive_push;
pub mod pull_repository;
pub mod resolve_repository_root;
pub(crate) mod working_tree;

fn git_failure(operation: &str, detail: &str) -> String {
    if detail.trim().is_empty() {
        format!("{operation} failed")
    } else {
        format!("{operation} failed: {}", detail.trim())
    }
}
