//! Concrete adapters (Ports & Adapters "outside"). Owns all storage/process/OS
//! integrations; the application core sees only ports. Currently: the
//! content-addressed diff store (formerly the standalone `gtl-store` crate) and
//! the git capture and clock adapters.

pub mod app_state;
pub mod artifact_store;
pub mod clock;
pub mod configured_editor;
pub mod data_root;
pub mod detached_process;
pub mod file_system;
mod git_capture;
pub mod git_client;
mod git_process;
pub mod managed_repo_client;
pub mod push_ledger;
pub mod repo_discovery;
pub mod store;
#[cfg(test)]
mod testing;
pub mod user_config;
