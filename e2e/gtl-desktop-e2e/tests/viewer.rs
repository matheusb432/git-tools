#![cfg(test)]

//! Release Tauri/WebKit journeys, one per critical user goal.

mod support;

#[path = "viewer/cli_review.rs"]
mod cli_review;

#[path = "viewer/diff_review_progress.rs"]
mod diff_review_progress;

#[path = "viewer/desktop_scroll_baseline.rs"]
mod desktop_scroll_baseline;

#[path = "viewer/desktop_shell.rs"]
mod desktop_shell;

#[path = "viewer/guided_tour.rs"]
mod guided_tour;

#[path = "viewer/large_diff.rs"]
mod large_diff;

#[path = "viewer/live_diff.rs"]
mod live_diff;

#[path = "viewer/projects.rs"]
mod projects;

#[path = "viewer/push.rs"]
mod push;

#[path = "viewer/settings.rs"]
mod settings;

#[path = "viewer/tabs.rs"]
mod tabs;
