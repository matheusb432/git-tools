#![cfg(test)]

//! Native Tauri/WebKit browser journeys.

mod support;

#[path = "viewer/live_lifecycle.rs"]
mod live_lifecycle;

#[path = "viewer/one_shot_lifecycle.rs"]
mod one_shot_lifecycle;

#[path = "viewer/raw_artifact.rs"]
mod raw_artifact;
