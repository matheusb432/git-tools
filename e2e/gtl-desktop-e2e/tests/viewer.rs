#![cfg(test)]

//! Native Tauri/WebKit browser journeys.

mod support;

#[path = "viewer/desktop_scroll_baseline.rs"]
mod desktop_scroll_baseline;

#[path = "viewer/live_lifecycle.rs"]
mod live_lifecycle;

#[path = "viewer/one_shot_lifecycle.rs"]
mod one_shot_lifecycle;
