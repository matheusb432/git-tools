#![cfg(test)]

//! Playwright browser journeys.
//!
//! Every workflow is a module of this explicit Cargo test target. The modules
//! share test-only mechanics while each spec owns its browser session and
//! disposable production fixture.

#[path = "browser/support.rs"]
mod support;

#[path = "browser/raw_artifact_lifecycle.rs"]
mod raw_artifact_lifecycle;
