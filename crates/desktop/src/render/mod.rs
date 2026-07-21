mod document;
mod fragments;
mod routes;

pub(crate) use document::MaudViewerRenderer;
use routes::{ViewerRoute, ViewerSettingChange};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application::{
        diffs::{Cmd, Foot, View},
        viewer::{
            DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, Theme, ViewerDocument,
            ViewerHistoryEntry, ViewerSettings, ViewerTab, ViewerTabId, ViewerTabKind,
            ViewerTabState, ViewerView,
        },
    };

    use super::{MaudViewerRenderer, ViewerRoute, ViewerSettingChange};

    const HTMX_SHA256: &str = "71ea67185bfa8c98c39d31717c6fce5d852370fcdfd129db4543774d3145c0de";

    fn tab_id(raw: u64) -> ViewerTabId {
        ViewerTabId::try_new(raw).expect("positive tab id")
    }

    fn history_id(raw: i64) -> RenderHistoryId {
        RenderHistoryId::try_new(raw).expect("positive history id")
    }

    fn view() -> Arc<View> {
        Arc::new(View {
            exclusions: None,
            repo_name: "git-tools".into(),
            repo_root: "/repo".into(),
            branch: "feature/htmx".into(),
            upstream: "origin/main".into(),
            commits: vec![],
            files: vec![],
            title: "Working tree".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: "Commits".into(),
            foot: Foot {
                cmd: "git diff".into(),
                note: String::new(),
            },
            theme: None,
        })
    }

    fn settings() -> ViewerSettings {
        ViewerSettings::new(
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            Theme::Hearth,
        )
    }

    fn sample_document() -> ViewerDocument {
        let id = tab_id(1);
        ViewerDocument::new(
            vec![ViewerTab::new(
                id,
                "git-tools changes".into(),
                ViewerTabKind::Live,
                ViewerTabState::Ready,
            )],
            Some(id),
            Some(ViewerView::new(
                id,
                view(),
                settings().options(),
                ViewerTabKind::Live,
            )),
            vec![ViewerHistoryEntry::new(
                history_id(7),
                "Recent changes".into(),
                "git-tools".into(),
                "worktree".into(),
                "main..HEAD".into(),
                "2026-07-11T00:00:00Z".into(),
            )],
            settings(),
        )
        .expect("sample document is internally consistent")
    }

    #[test]
    fn route_formatter_is_closed_over_validated_values() {
        let tab = tab_id(7);
        let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
        let render = history_id(42);

        assert_eq!(
            ViewerRoute::View { tab, options }.to_string(),
            "/tabs/7/view?layout=split&density=full"
        );
        assert_eq!(
            ViewerRoute::Activate { tab }.to_string(),
            "/tabs/7/activate"
        );
        assert_eq!(ViewerRoute::Refresh { tab }.to_string(), "/tabs/7/refresh");
        assert_eq!(ViewerRoute::Close { tab }.to_string(), "/tabs/7/close");
        assert_eq!(
            ViewerRoute::DeleteLiveView { tab }.to_string(),
            "/tabs/7/live-view"
        );
        assert_eq!(ViewerRoute::History.to_string(), "/history");
        assert_eq!(
            ViewerRoute::OpenHistory { render }.to_string(),
            "/history/42/open"
        );
        assert_eq!(
            ViewerRoute::Settings(ViewerSettingChange::Layout(DiffLayout::Unified)).to_string(),
            "/settings?layout=unified"
        );
        assert_eq!(
            ViewerRoute::Settings(ViewerSettingChange::Density(DiffDensity::Compact)).to_string(),
            "/settings?density=compact"
        );
        assert_eq!(
            ViewerRoute::Settings(ViewerSettingChange::Theme(Theme::Dark)).to_string(),
            "/settings?theme=dark"
        );
    }

    #[test]
    fn document_is_offline_and_has_stable_swap_roots_and_one_active_layout() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let expected_route = ViewerRoute::View {
            tab: tab_id(1),
            options: settings().options(),
        }
        .to_string();

        assert!(html.contains("id=\"viewer-tabs\""));
        assert!(html.contains("id=\"viewer-view\""));
        assert!(html.contains("id=\"viewer-history\""));
        assert!(html.contains(&expected_route.replace('&', "&amp;")));
        assert_eq!(html.matches(crate::protocol_config::APP_URL).count(), 1);
        assert!(!html.contains("<iframe"));
        assert!(!html.contains("<script src="));
        assert!(!html.contains("<link "));
        assert!(!html.contains("hx-swap-oob="));
        assert_eq!(html.matches("class=\"layout").count(), 1);
    }

    #[test]
    fn user_controlled_shell_values_are_escaped() {
        let tab = ViewerTab::new(
            tab_id(1),
            "<script>alert(1)</script>".into(),
            ViewerTabKind::Snapshot,
            ViewerTabState::Error {
                reason: "failed".into(),
            },
        );

        let html = MaudViewerRenderer.build_tabs(&[tab], None);

        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn broken_and_error_tabs_render_distinct_escaped_recovery_states() {
        for (state, marker) in [
            (
                ViewerTabState::Broken {
                    code: "missing<&>".into(),
                    reason: "artifact <gone>".into(),
                },
                "viewer-status-broken",
            ),
            (
                ViewerTabState::Error {
                    reason: "render <failed>".into(),
                },
                "viewer-status-error",
            ),
        ] {
            let id = tab_id(3);
            let document = ViewerDocument::new(
                vec![ViewerTab::new(
                    id,
                    "unavailable".into(),
                    ViewerTabKind::Live,
                    state,
                )],
                Some(id),
                None,
                vec![],
                settings(),
            )
            .expect("non-ready active tabs intentionally have no view");

            let html = MaudViewerRenderer.build_document(&document);

            assert!(html.contains(marker));
            assert!(html.contains("&lt;"));
            assert!(!html.contains("class=\"layout"));
            assert!(html.contains("/tabs/3/refresh"));
            assert!(html.contains("hx-delete=\"/tabs/3/live-view\""));
            assert!(html.contains("hx-confirm=\"Delete this saved live view?"));
        }
    }

    #[test]
    fn destructive_live_action_is_accessible_and_never_appears_for_snapshots_or_raw_artifacts() {
        let live = MaudViewerRenderer.build_document(&sample_document());
        assert!(live.contains("class=\"viewer-control-button viewer-danger-button\""));
        assert!(live.contains("aria-label=\"Delete saved live view\""));
        assert!(live.contains("hx-delete=\"/tabs/1/live-view\""));
        assert!(live.contains("hx-target=\"#viewer-tabs\""));

        let id = tab_id(1);
        let snapshot = ViewerDocument::new(
            vec![ViewerTab::new(
                id,
                "snapshot".into(),
                ViewerTabKind::Snapshot,
                ViewerTabState::Ready,
            )],
            Some(id),
            Some(ViewerView::new(
                id,
                view(),
                settings().options(),
                ViewerTabKind::Snapshot,
            )),
            vec![],
            settings(),
        )
        .expect("snapshot document is valid");
        let snapshot = MaudViewerRenderer.build_document(&snapshot);
        assert!(!snapshot.contains("Delete live view"));
        assert!(!snapshot.contains("/live-view"));

        let raw = preview::build_html(&view());
        assert!(!raw.contains("Delete live view"));
        assert!(!raw.contains("/live-view"));
    }

    #[test]
    fn empty_document_gives_a_direct_next_step() {
        let document = ViewerDocument::new(vec![], None, None, vec![], settings())
            .expect("empty viewer is valid");

        let html = MaudViewerRenderer.build_document(&document);

        assert!(html.contains("viewer-status-empty"));
        assert!(html.contains("gtl diff"));
        assert!(!html.contains("class=\"layout"));
    }

    #[test]
    fn history_uses_stable_ids_and_escapes_metadata() {
        let entry = ViewerHistoryEntry::new(
            history_id(9),
            "<recent>".into(),
            "repo & tools".into(),
            "worktree".into(),
            "main..HEAD".into(),
            "2026-07-11T00:00:00Z".into(),
        );

        let html = MaudViewerRenderer.build_history(&[entry]);
        let row = html
            .split_once("<button")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("history entry renders as a button");

        assert!(html.contains("/history/9/open"));
        assert!(html.contains("&lt;recent&gt;"));
        assert!(html.contains("repo &amp; tools"));
        assert!(row.contains("popovertarget=\"viewer-history-popover\""));
        assert!(row.contains("popovertargetaction=\"hide\""));
    }

    #[test]
    fn theme_setting_updates_the_document_palette_without_a_reload() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        assert!(html.contains("data-viewer-theme=\"dark\""));
        assert!(html.contains("document.documentElement.dataset.theme"));
        assert!(html.contains("/settings?theme=dark"));
    }

    #[test]
    fn layout_and_density_swaps_follow_native_radio_changes() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        assert_eq!(
            html.matches("hx-trigger=\"change from:find input\"")
                .count(),
            4
        );
    }

    #[test]
    fn settings_controls_exclude_native_radio_parameters() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let settings_inputs: Vec<&str> = html
            .split("<input ")
            .skip(1)
            .map(|tail| tail.split_once('>').expect("input has a closing angle").0)
            .filter(|input| input.contains("hx-get=\"/settings?"))
            .collect();

        assert_eq!(settings_inputs.len(), 7);
        for input in settings_inputs {
            assert!(input.contains("hx-params=\"none\""), "{input}");
        }
    }

    #[test]
    fn view_primary_response_updates_tabs_out_of_band() {
        let html = MaudViewerRenderer.build_view_with_tabs(&sample_document());

        assert_eq!(html.matches("id=\"viewer-view\"").count(), 1);
        assert_eq!(html.matches("id=\"viewer-tabs\"").count(), 1);
        assert!(html.starts_with("<section id=\"viewer-view\""));
        assert!(html.contains("<nav id=\"viewer-tabs\" hx-swap-oob=\"outerHTML\""));
        assert!(!html.starts_with("<section id=\"viewer-view\" hx-swap-oob"));
    }

    #[test]
    fn tabs_primary_response_updates_view_out_of_band() {
        let html = MaudViewerRenderer.build_tabs_with_view(&sample_document());

        assert_eq!(html.matches("id=\"viewer-tabs\"").count(), 1);
        assert_eq!(html.matches("id=\"viewer-view\"").count(), 1);
        assert!(html.starts_with("<nav id=\"viewer-tabs\""));
        assert!(html.contains("<section id=\"viewer-view\" hx-swap-oob=\"outerHTML\""));
        assert!(!html.starts_with("<nav id=\"viewer-tabs\" hx-swap-oob"));
    }

    #[test]
    fn snapshot_skips_render_one_accessible_escaped_toast() {
        let labels = vec!["api".into(), "<script>web</script>".into()];

        let html = MaudViewerRenderer
            .build_tabs_with_view_after_snapshot_skips(&sample_document(), &labels);

        assert!(html.contains("class=\"gtl-toast viewer-toast-skip show\""));
        assert!(html.contains("data-viewer-toast"));
        assert!(html.contains("role=\"status\" aria-live=\"polite\" aria-atomic=\"true\""));
        assert!(html.contains(
            "Skipped 2 diffs with no commits or changed files: api, &lt;script&gt;web&lt;/script&gt;."
        ));
        assert!(!html.contains("<script>web</script>"));
    }

    #[test]
    fn snapshot_skip_toast_has_a_bounded_reduced_motion_aware_lifetime() {
        let css = super::document::viewer_css();

        assert!(css.contains("@keyframes viewer-toast-dismiss"));
        assert!(css.contains("animation:5s forwards viewer-toast-dismiss"));
        assert!(css.contains("@media (prefers-reduced-motion:reduce)"));
        assert!(css.contains("animation-timing-function:step-end"));
    }

    #[test]
    fn live_delete_feedback_focuses_the_active_tab_and_announces_success() {
        let html = MaudViewerRenderer.build_tabs_with_view_after_live_delete(&sample_document());
        let active = html
            .split_once("class=\"viewer-tab-activate\"")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("active tab button renders");

        assert!(active.contains("autofocus"), "{active}");
        assert!(html.contains("role=\"status\" aria-live=\"polite\" aria-atomic=\"true\""));
        assert!(html.contains("Live view deleted. Focus moved to git-tools changes."));
        assert_eq!(html.matches("autofocus").count(), 1);
    }

    #[test]
    fn live_delete_feedback_focuses_history_when_no_tabs_remain() {
        let empty = ViewerDocument::new(vec![], None, None, vec![], settings())
            .expect("empty viewer is valid");
        let html = MaudViewerRenderer.build_tabs_with_view_after_live_delete(&empty);
        let history = html
            .split_once("class=\"viewer-recovery-button\"")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("empty-state History action renders");

        assert!(history.contains("autofocus"), "{history}");
        assert!(html.contains("Live view deleted. No diffs remain open."));
        assert!(html.contains("Open History or run gtl diff live to add one."));
        assert_eq!(html.matches("autofocus").count(), 1);
    }

    #[test]
    fn narrow_app_split_rows_stack_without_changing_raw_artifacts() {
        let viewer = super::document::viewer_css();
        let shared = preview::preview_css();

        // the app-only stacking rules live in the viewer sheet...
        assert!(viewer.contains(
            "body.viewer-shell .diff-split .dl{grid-template-columns:44px minmax(0,1fr)}"
        ));
        assert!(viewer.contains(
            "body.viewer-shell .diff-split .dl-meta,body.viewer-shell .diff-split .dl-hunk{grid-template-columns:minmax(0,1fr)}"
        ));
        assert!(!shared.contains("body.viewer-shell"));
        // ...while the artifact narrow-screen pane fallback stays in the shared sheet
        assert!(shared.contains("body:not(.viewer-shell) .diff-unified.diff-compact"));
        assert!(!viewer.contains("body:not(.viewer-shell)"));
    }

    #[test]
    fn open_diff_navigation_uses_buttons_without_partial_tab_aria() {
        let document = sample_document();
        let html = MaudViewerRenderer.build_tabs(document.tabs(), Some(tab_id(1)));

        assert!(html.contains("<nav id=\"viewer-tabs\""));
        assert!(html.contains("<ul class=\"viewer-tab-list\""));
        assert!(html.contains("aria-current=\"page\""));
        assert!(!html.contains("role=\"tablist\""));
        assert!(!html.contains("role=\"tab\""));
        assert!(!html.contains("aria-selected="));
    }

    #[test]
    fn dependent_actions_target_their_primary_compound_root() {
        fn opening_tag_with<'html>(html: &'html str, needle: &str) -> &'html str {
            let position = html.find(needle).expect("route appears in rendered markup");
            let start = html[..position].rfind('<').expect("route belongs to a tag");
            let end = html[position..].find('>').expect("tag closes") + position + 1;
            &html[start..end]
        }

        let document = sample_document();
        let html = MaudViewerRenderer.build_document(&document);
        let activate = opening_tag_with(&html, "/tabs/1/activate");
        let close = opening_tag_with(&html, "/tabs/1/close");
        let open_history = opening_tag_with(&html, "/history/7/open");

        assert!(activate.contains("hx-target=\"#viewer-view\""));
        for tag in [close, open_history] {
            assert!(tag.contains("hx-target=\"#viewer-tabs\""), "{tag}");
        }
    }

    #[test]
    fn recipe_wake_subscription_precedes_the_first_pending_drain() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let listen = html
            .find("event.listen(\"recipes-pending\"")
            .expect("shell subscribes to recipe wake events");
        let pending = html.find("/pending").expect("shell drains pending recipes");

        assert!(
            listen < pending,
            "subscription must be established before draining"
        );
        assert!(html.contains("await window.__TAURI__.event.listen"));
        assert!(html.contains("var chain=Promise.resolve()"));
        assert!(!html.contains("hx-trigger=\"load\""));
        assert!(!html.contains("<iframe"));
    }

    #[test]
    fn vendored_htmx_digest_is_pinned() {
        use std::fmt::Write as _;

        use sha2::{Digest as _, Sha256};

        let digest = Sha256::digest(crate::protocol_config::HTMX.as_bytes());
        let mut encoded = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(encoded, "{byte:02x}").expect("writing to a string cannot fail");
        }

        assert_eq!(encoded, HTMX_SHA256);
    }
}
