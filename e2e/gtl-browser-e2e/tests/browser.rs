//! Playwright browser journeys.
//!
//! Every workflow is a module of this explicit Cargo test target. The modules
//! share test-only mechanics while each spec owns its browser session and
//! disposable production fixture.

#[path = "support.rs"]
mod harness;

#[path = "browser/support.rs"]
mod support;

#[path = "browser/raw_artifact_lifecycle.rs"]
mod raw_artifact_lifecycle;

#[path = "browser/component_preview_registry.rs"]
mod component_preview_registry;

#[path = "browser/review_controls.rs"]
mod review_controls;
