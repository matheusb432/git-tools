//! Concrete adapters for storage, process, filesystem, clock, and OS integration.

pub mod app_state;
pub mod artifact_store;
pub mod clock;
pub mod data_root;
pub mod detached_process;
pub mod file_system;
mod git_capture;
pub mod git_client;
mod git_process;
mod history_projects;
mod project_comparison_reader;
pub mod project_repository_client;
pub mod store;
#[cfg(test)]
mod testing;
pub mod text_editor;
pub mod user_config;
