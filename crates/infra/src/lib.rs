//! Concrete adapters (Ports & Adapters "outside"). Owns all storage/process/OS
//! integrations; the application core sees only ports. Currently: the
//! content-addressed diff store (formerly the standalone `gtl-store` crate), the
//! git capture, clock, and Maud renderer adapters.

pub mod app_state;
pub mod artifact_store;
pub mod clock;
mod comment_syntax;
pub mod diff_source;
pub mod git_capture;
pub mod html_renderer;
pub mod managed_manifest;
pub mod push_ledger;
pub mod remote_sync;
pub mod repo_probe;
pub mod store;
