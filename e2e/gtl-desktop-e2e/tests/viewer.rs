#![cfg(test)]

//! Native Tauri/WebKit browser journeys.

mod support;

#[path = "viewer/desktop_scroll_baseline.rs"]
mod desktop_scroll_baseline;

#[path = "viewer/live_lifecycle.rs"]
mod live_lifecycle;

#[path = "viewer/one_shot_lifecycle.rs"]
mod one_shot_lifecycle;

#[path = "viewer/tab_overflow.rs"]
mod tab_overflow;

#[path = "viewer/projects.rs"]
mod projects;

#[path = "viewer/settings_recovery.rs"]
mod settings_recovery;

#[path = "viewer/line_wrapping.rs"]
mod line_wrapping;

#[path = "viewer/desktop_shell.rs"]
mod desktop_shell;
