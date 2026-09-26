mod support;

use dioxus::prelude::{EventHandler, ScopeId, VNode, VirtualDom};
use gtl_models::diffs::CommitId;
use gtl_web::{CommitsPanel, CommitsPanelProps};
use gtl_web_contracts::test_ids;
use support::{TestResult, viewer_active_view, viewer_commit_summary};

#[test]
fn single_commit_panel_is_read_only_but_keeps_copy_action() -> TestResult {
    let view = viewer_active_view(vec![viewer_commit_summary()?])?;
    let event_handler_owner = VirtualDom::new(VNode::empty);
    let props = event_handler_owner.in_scope(ScopeId::ROOT, || CommitsPanelProps {
        view,
        test_id: Some(test_ids::COMMITS_PANEL.value().to_owned()),
        onselect: Some(EventHandler::<CommitId>::new(|_| {})),
        loading: false,
        load_error: None,
        has_more: false,
        onloadmore: None,
    });
    let mut panel = VirtualDom::new_with_props(CommitsPanel, props);
    panel.rebuild_in_place();

    let html = dioxus_ssr::render(&panel);

    assert!(html.contains("<article"));
    assert!(!html.contains("aria-pressed"));
    assert_eq!(html.matches("<button").count(), 1);
    assert!(html.contains("title=\"Copy commit ID\""));
    assert!(!html.contains("data-testid=\"commit-details-trigger\""));
    assert!(html.contains("data-gtl-hover-popover-target=\"\""));
    assert!(html.contains("data-gtl-hover-popover-delay-ms=\"350\""));
    assert!(html.contains("aria-label=\"Commit details for 0123456789\""));
    assert!(html.contains("popover=\"auto\""));
    assert!(html.contains("hover-popover-content"));
    let stylesheet = include_str!("../src/app/assets/styles/overlays.css");
    let (_, styles) = stylesheet.split_once(".hover-popover-content {").unwrap();
    let styles = styles.split('}').next().unwrap();
    assert!(styles.contains("animate-commit-popover-enter"));
    assert!(styles.contains("overflow-x-hidden"));
    assert!(html.contains("2026-08-19T10:00:00Z"));
    assert!(html.contains("data-testid=\"commits-panel\""));
    Ok(())
}

#[test]
fn commit_details_content_waits_for_hover() -> TestResult {
    let mut commit = viewer_commit_summary()?;
    commit.body = "Explain the implementation constraints.".to_owned();
    let view = viewer_active_view(vec![commit])?;
    let props = CommitsPanelProps {
        view,
        test_id: None,
        onselect: None,
        loading: false,
        load_error: None,
        has_more: false,
        onloadmore: None,
    };
    let mut panel = VirtualDom::new_with_props(CommitsPanel, props);
    panel.rebuild_in_place();

    let html = dioxus_ssr::render(&panel);

    assert!(!html.contains("Explain the implementation constraints."));
    assert!(html.contains("Commit details"));
    assert!(!html.contains(">Date</dt>"));
    assert!(!html.contains(">Committed</dt>"));
    assert!(!html.contains(">2026-08-19T10:00:00Z<"));
    assert!(html.contains("Copy commit ID"));
    Ok(())
}
