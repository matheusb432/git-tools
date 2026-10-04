//! Concrete adapters for storage, process, filesystem, clock, and OS integration.

pub mod app_state;
pub mod artifact_store;
pub mod clock;
pub mod data_root;
pub mod detached_process;
mod diff_review_reader;
mod diff_text_reader;
mod extension_filter_store;
pub mod file_system;
mod git_capture;
pub mod git_client;
mod git_process;
mod history_projects;
mod project_comparison_reader;
pub mod project_repository_client;
pub mod project_status_watch;
pub mod store;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod text_editor;
pub mod user_config;
