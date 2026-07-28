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
pub mod diff_source;
pub mod file_system;
pub mod git_capture;
pub mod git_runner;
pub mod managed_manifest;
pub mod push_ledger;
pub mod remote_sync;
pub mod repo_discovery;
pub mod repo_probe;
pub mod store;
pub mod user_config;
