#![cfg(any(feature = "artifact", feature = "desktop"))]

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
        onclear: None,
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
    assert!(html.contains("data-testid=\"commits-panel\""));
    Ok(())
}
